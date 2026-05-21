//! Leveled compaction: merge L0 SSTables into L1 (Week 15).

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::StorageError;
use crate::memtable::MemTable;
use crate::sstable::{SstReader, SstWriter};

/// Trigger compaction when L0 reaches this many files.
pub const L0_COMPACTION_TRIGGER: usize = 4;

type MergeEntry = Reverse<(Vec<u8>, Vec<u8>, usize)>;

/// Merge all L0 SSTables into a single L1 file. Returns the new L1 path.
pub fn compact_level0_to_l1(
    data_dir: impl AsRef<Path>,
    l0_files: &[PathBuf],
) -> Result<PathBuf, StorageError> {
    if l0_files.is_empty() {
        return Err(StorageError::invalid_input("nothing to compact"));
    }

    let mut heap: BinaryHeap<MergeEntry> = BinaryHeap::new();
    let iters: Vec<_> = l0_files
        .iter()
        .map(SstReader::open)
        .collect::<Result<Vec<_>, _>>()?;
    let mut streams: Vec<_> = iters
        .iter()
        .map(SstReader::scan)
        .collect::<Result<Vec<_>, _>>()?;

    for (i, iter) in streams.iter_mut().enumerate() {
        if let Some(Ok((k, v))) = iter.next() {
            heap.push(Reverse((k, v, i)));
        }
    }

    let mut merged = MemTable::new();
    while let Some(Reverse((k, v, i))) = heap.pop() {
        merged.put(&k, &v)?;
        if let Some(Ok((nk, nv))) = streams[i].next() {
            heap.push(Reverse((nk, nv, i)));
        }
    }

    let sst_dir = data_dir.as_ref().join("sst");
    fs::create_dir_all(&sst_dir)?;
    let id = next_sst_id(&sst_dir, 1)?;
    let out = sst_dir.join(format!("L1-{id:09}.sst"));
    SstWriter::write_from_memtable(&out, &merged)?;

    for f in l0_files {
        fs::remove_file(f)?;
    }

    Ok(out)
}

fn next_sst_id(dir: &Path, level: u32) -> Result<u64, StorageError> {
    let prefix = format!("L{level}-");
    let mut max_id = 0u64;
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let name = entry?.file_name();
            if let Some(s) = name.to_str() {
                if let Some(id_str) = s.strip_prefix(&prefix).and_then(|s| s.strip_suffix(".sst")) {
                    if let Ok(id) = id_str.parse::<u64>() {
                        max_id = max_id.max(id);
                    }
                }
            }
        }
    }
    Ok(max_id + 1)
}

pub(crate) fn list_sst_level(dir: &Path, level: u32) -> Result<Vec<PathBuf>, StorageError> {
    let prefix = format!("L{level}-");
    let mut files = Vec::new();
    if !dir.exists() {
        return Ok(files);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(&prefix)
                && Path::new(name)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sst"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(crate) fn alloc_sst_path(dir: &Path, level: u32) -> Result<PathBuf, StorageError> {
    fs::create_dir_all(dir)?;
    let id = next_sst_id(dir, level)?;
    Ok(dir.join(format!("L{level}-{id:09}.sst")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::memtable::MemTable;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-compact-{nanos}"))
    }

    #[test]
    fn merge_five_l0_into_one_l1() {
        let dir = temp_dir();
        let sst_dir = dir.join("sst");
        fs::create_dir_all(&sst_dir).unwrap();

        let mut l0 = Vec::new();
        for batch in 0..5u32 {
            let mut table = MemTable::new();
            for i in 0..100u32 {
                let k = format!("b{batch}:k:{i:03}");
                table.put(k.as_bytes(), b"v").unwrap();
            }
            let path = sst_dir.join(format!("L0-{batch:09}.sst"));
            SstWriter::write_from_memtable(&path, &table).unwrap();
            l0.push(path);
        }

        let l1 = compact_level0_to_l1(&dir, &l0).unwrap();
        assert!(l1.exists());
        let reader = SstReader::open(&l1).unwrap();
        assert_eq!(reader.get(b"b0:k:000").unwrap(), Some(b"v".to_vec()));
        assert!(list_sst_level(&sst_dir, 0).unwrap().is_empty());
        let _ = fs::remove_dir_all(dir);
    }
}
