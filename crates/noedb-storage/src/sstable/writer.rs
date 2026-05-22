//! SSTable writer — sorted KV blocks + index + bloom (Weeks 12–14).

use std::fs::File;
use std::io::Write;
use std::path::Path;

use super::format::{Footer, IndexEntry, BLOCK_SIZE, HEADER_LEN, SST_MAGIC, SST_VERSION};
use super::options::SstWriteOptions;
use crate::bloom::BloomFilter;
use crate::error::StorageError;
use crate::memtable::MemTable;

/// Writes an immutable SSTable from a sorted [`MemTable`].
pub struct SstWriter {
    file: File,
    offset: u64,
}

impl SstWriter {
    /// Write with Phase 3 options (currently maps to v1 Bloom SST; LZ4 path follows).
    pub fn write_from_memtable_opts(
        path: impl AsRef<Path>,
        table: &MemTable,
        opts: SstWriteOptions,
    ) -> Result<u64, StorageError> {
        let _ = opts;
        Self::write_from_memtable(path, table)
    }

    /// Write `table` to `path` and return the number of keys written.
    pub fn write_from_memtable(
        path: impl AsRef<Path>,
        table: &MemTable,
    ) -> Result<u64, StorageError> {
        let path = path.as_ref();
        let mut file = File::create(path)?;
        file.write_all(&SST_MAGIC)?;
        file.write_all(&SST_VERSION.to_le_bytes())?;
        file.write_all(&(BLOCK_SIZE as u32).to_le_bytes())?;
        file.write_all(&0u32.to_le_bytes())?; // reserved
        file.write_all(&0u16.to_le_bytes())?; // pad to HEADER_LEN

        let mut writer = Self {
            file,
            offset: HEADER_LEN as u64,
        };
        let mut index = Vec::new();
        let mut block = Vec::with_capacity(BLOCK_SIZE);
        let mut keys_for_bloom = Vec::new();
        let mut key_count = 0u64;

        for (k, v) in table.iter() {
            keys_for_bloom.push(k.clone());
            key_count += 1;

            if block.is_empty() {
                index.push(IndexEntry {
                    first_key: k.clone(),
                    offset: writer.offset,
                });
            }

            let record = encode_record(&k, &v);
            if !block.is_empty() && block.len() + record.len() > BLOCK_SIZE {
                writer.write_block(&block)?;
                block.clear();
                index.push(IndexEntry {
                    first_key: k,
                    offset: writer.offset,
                });
            }
            block.extend_from_slice(&record);
        }

        if !block.is_empty() {
            writer.write_block(&block)?;
        }

        let bloom = BloomFilter::build(keys_for_bloom.iter(), keys_for_bloom.len().max(1), 0.01);
        let bloom_offset = writer.offset;
        let bloom_bytes = bloom.encode();
        writer.file.write_all(&bloom_bytes)?;
        writer.offset += bloom_bytes.len() as u64;

        let index_offset = writer.offset;
        for entry in &index {
            let key_len = entry.first_key.len() as u16;
            writer.file.write_all(&key_len.to_le_bytes())?;
            writer.file.write_all(&entry.first_key)?;
            writer.file.write_all(&entry.offset.to_le_bytes())?;
            writer.offset += 2 + entry.first_key.len() as u64 + 8;
        }

        let footer = Footer {
            index_offset,
            index_count: index.len() as u32,
            bloom_offset,
        };
        write_footer(&mut writer.file, &footer)?;
        writer.file.sync_all()?;
        Ok(key_count)
    }

    fn write_block(&mut self, block: &[u8]) -> Result<(), StorageError> {
        let len = block.len() as u32;
        self.file.write_all(&len.to_le_bytes())?;
        self.file.write_all(block)?;
        self.offset += 4 + block.len() as u64;
        Ok(())
    }
}

fn encode_record(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + key.len() + value.len());
    out.extend_from_slice(&(key.len() as u16).to_le_bytes());
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(value);
    out
}

fn write_footer(file: &mut File, footer: &Footer) -> Result<(), StorageError> {
    file.write_all(&footer.index_offset.to_le_bytes())?;
    file.write_all(&footer.index_count.to_le_bytes())?;
    file.write_all(&footer.bloom_offset.to_le_bytes())?;
    file.write_all(&0u32.to_le_bytes())?; // reserved
    file.write_all(&SST_MAGIC)?;
    file.write_all(&SST_VERSION.to_le_bytes())?;
    file.write_all(&0u16.to_le_bytes())?; // pad to FOOTER_LEN
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_sst() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-sst-{nanos}.sst"))
    }

    #[test]
    fn writes_fifty_k_entries() {
        let path = temp_sst();
        let mut table = MemTable::new();
        for i in 0..50_000u32 {
            let k = format!("k:{i:06}");
            table.put(k.as_bytes(), b"v").unwrap();
        }
        let n = SstWriter::write_from_memtable(&path, &table).unwrap();
        assert_eq!(n, 50_000);
        let meta = std::fs::metadata(&path).unwrap();
        assert!(meta.len() > super::super::format::FOOTER_LEN as u64);
        let _ = std::fs::remove_file(path);
    }
}
