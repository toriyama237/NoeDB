//! SSTable reader — index lookup + block scan (Week 13).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::block::decode_block_at;
use super::format::{Footer, IndexEntry, FOOTER_LEN, HEADER_LEN, SST_MAGIC, SST_VERSION, SST_VERSION_V2, SST_VERSION_V3};
use crate::bloom::BloomFilter;
use crate::error::StorageError;
use crate::mmap_io::{map_read_only, MappedFile};

/// Read-only handle to an on-disk SSTable.
#[derive(Debug, Clone)]
pub struct SstReader {
    path: PathBuf,
    index: Vec<IndexEntry>,
    bloom: BloomFilter,
    mmap: Option<Arc<MappedFile>>,
    checksum_blocks: bool,
}

impl SstReader {
    /// Open and parse `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path)?;

        let mut magic = [0u8; 4];
        file.read_exact(&mut magic)?;
        if magic != SST_MAGIC {
            return Err(StorageError::corrupt_sstable("bad SST magic"));
        }
        let mut ver = [0u8; 2];
        file.read_exact(&mut ver)?;
        let version = u16::from_le_bytes(ver);
        if version != SST_VERSION && version != SST_VERSION_V2 && version != SST_VERSION_V3 {
            return Err(StorageError::corrupt_sstable("unsupported SST version"));
        }
        let checksum_blocks = version >= SST_VERSION_V3;

        let footer = read_footer(&mut file)?;
        let index = read_index(&mut file, &footer)?;
        let bloom = read_bloom(&mut file, &footer)?;

        let mmap = map_read_only(&path).ok().map(Arc::new);
        Ok(Self {
            path,
            index,
            bloom,
            mmap,
            checksum_blocks,
        })
    }

    /// Verify every indexed block checksum (background scrub helper).
    pub fn verify_all_blocks(&self) -> Result<usize, StorageError> {
        let mut count = 0usize;
        for entry in &self.index {
            self.read_block(entry.offset)?;
            count += 1;
        }
        Ok(count)
    }

    /// Path to the underlying file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Look up `key` with bloom short-circuit + O(log n) index probe.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if !self.bloom.may_contain(key) {
            return Ok(None);
        }

        let Some(block_idx) = self.find_block(key) else {
            return Ok(None);
        };

        let block = self.read_block(self.index[block_idx].offset)?;
        Ok(scan_block(&block, key))
    }

    /// Full-table scan in key order.
    pub fn scan(&self) -> Result<SstIter, StorageError> {
        SstIter::new(self)
    }

    fn find_block(&self, key: &[u8]) -> Option<usize> {
        if self.index.is_empty() {
            return None;
        }
        let mut lo = 0usize;
        let mut hi = self.index.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.index[mid].first_key.as_slice() <= key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == 0 {
            None
        } else {
            Some(lo - 1)
        }
    }

    fn read_block(&self, offset: u64) -> Result<Vec<u8>, StorageError> {
        if let Some(mmap) = &self.mmap {
            return decode_block_at(mmap.as_slice(), offset, self.checksum_blocks);
        }
        let bytes = std::fs::read(&self.path)?;
        decode_block_at(&bytes, offset, self.checksum_blocks)
    }

    pub(crate) fn index(&self) -> &[IndexEntry] {
        &self.index
    }

    pub(crate) fn read_block_at(&self, offset: u64) -> Result<Vec<u8>, StorageError> {
        self.read_block(offset)
    }
}

fn read_footer(file: &mut File) -> Result<Footer, StorageError> {
    let len = file.metadata()?.len();
    if len < (HEADER_LEN + FOOTER_LEN) as u64 {
        return Err(StorageError::corrupt_sstable("file too small"));
    }
    file.seek(SeekFrom::End(-i64::try_from(FOOTER_LEN).map_err(|_| {
        StorageError::corrupt_sstable("footer length overflow")
    })?))?;
    let mut buf = [0u8; FOOTER_LEN];
    file.read_exact(&mut buf)?;

    let index_offset = u64::from_le_bytes(
        buf[0..8]
            .try_into()
            .map_err(|_| StorageError::corrupt_sstable("bad footer index offset"))?,
    );
    let index_count = u32::from_le_bytes(
        buf[8..12]
            .try_into()
            .map_err(|_| StorageError::corrupt_sstable("bad footer index count"))?,
    );
    let bloom_offset = u64::from_le_bytes(
        buf[12..20]
            .try_into()
            .map_err(|_| StorageError::corrupt_sstable("bad footer bloom offset"))?,
    );
    let magic = &buf[24..28];
    if magic != SST_MAGIC {
        return Err(StorageError::corrupt_sstable("bad footer magic"));
    }

    Ok(Footer {
        index_offset,
        index_count,
        bloom_offset,
    })
}

fn read_index(file: &mut File, footer: &Footer) -> Result<Vec<IndexEntry>, StorageError> {
    file.seek(SeekFrom::Start(footer.index_offset))?;
    let mut index = Vec::with_capacity(footer.index_count as usize);
    for _ in 0..footer.index_count {
        let mut key_len_buf = [0u8; 2];
        file.read_exact(&mut key_len_buf)?;
        let key_len = u16::from_le_bytes(key_len_buf) as usize;
        let mut first_key = vec![0u8; key_len];
        file.read_exact(&mut first_key)?;
        let mut off_buf = [0u8; 8];
        file.read_exact(&mut off_buf)?;
        let offset = u64::from_le_bytes(off_buf);
        index.push(IndexEntry { first_key, offset });
    }
    Ok(index)
}

fn read_bloom(file: &mut File, footer: &Footer) -> Result<BloomFilter, StorageError> {
    file.seek(SeekFrom::Start(footer.bloom_offset))?;
    let bloom_len = footer.index_offset - footer.bloom_offset;
    let mut data = vec![0u8; bloom_len as usize];
    file.read_exact(&mut data)?;
    BloomFilter::decode(&data)
}

fn scan_block(block: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    let mut off = 0usize;
    while off + 6 <= block.len() {
        let key_len = u16::from_le_bytes(block[off..off + 2].try_into().ok()?) as usize;
        let val_len = u32::from_le_bytes(block[off + 2..off + 6].try_into().ok()?) as usize;
        off += 6;
        if off + key_len + val_len > block.len() {
            break;
        }
        let k = &block[off..off + key_len];
        off += key_len;
        let v = block[off..off + val_len].to_vec();
        off += val_len;
        if k == key {
            return Some(v);
        }
    }
    None
}

/// Iterator over all KV pairs in an SSTable.
pub struct SstIter {
    reader: SstReader,
    block_idx: usize,
    block: Vec<u8>,
    off: usize,
}

impl SstIter {
    fn new(reader: &SstReader) -> Result<Self, StorageError> {
        let mut iter = Self {
            reader: reader.clone(),
            block_idx: 0,
            block: Vec::new(),
            off: 0,
        };
        iter.load_block()?;
        Ok(iter)
    }

    fn load_block(&mut self) -> Result<(), StorageError> {
        if self.block_idx >= self.reader.index().len() {
            self.block.clear();
            self.off = 0;
            return Ok(());
        }
        let offset = self.reader.index()[self.block_idx].offset;
        self.block = self.reader.read_block_at(offset)?;
        self.off = 0;
        Ok(())
    }
}

impl Iterator for SstIter {
    type Item = Result<(Vec<u8>, Vec<u8>), StorageError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.block_idx >= self.reader.index().len() {
                return None;
            }
            if self.off + 6 > self.block.len() {
                self.block_idx += 1;
                if let Err(e) = self.load_block() {
                    return Some(Err(e));
                }
                continue;
            }
            let key_len =
                u16::from_le_bytes(self.block[self.off..self.off + 2].try_into().ok()?) as usize;
            let val_len =
                u32::from_le_bytes(self.block[self.off + 2..self.off + 6].try_into().ok()?)
                    as usize;
            self.off += 6;
            if self.off + key_len + val_len > self.block.len() {
                self.block_idx += 1;
                let _ = self.load_block();
                continue;
            }
            let key = self.block[self.off..self.off + key_len].to_vec();
            self.off += key_len;
            let val = self.block[self.off..self.off + val_len].to_vec();
            self.off += val_len;
            return Some(Ok((key, val)));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::memtable::MemTable;
    use crate::sstable::writer::SstWriter;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_sst() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-sst-r-{nanos}.sst"))
    }

    #[test]
    fn read_fifty_k_written_entries() {
        let path = temp_sst();
        let mut table = MemTable::new();
        for i in 0..50_000u32 {
            let k = format!("k:{i:06}");
            table.put(k.as_bytes(), b"v").unwrap();
        }
        SstWriter::write_from_memtable(&path, &table).unwrap();
        let reader = SstReader::open(&path).unwrap();
        for i in 0..50_000u32 {
            let k = format!("k:{i:06}");
            assert_eq!(reader.get(k.as_bytes()).unwrap(), Some(b"v".to_vec()));
        }
        let count = reader.scan().unwrap().count();
        assert_eq!(count, 50_000);
        let _ = std::fs::remove_file(path);
    }
}
