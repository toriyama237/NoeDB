//! Full LSM-tree engine orchestrating MemTable, WAL, SSTables (Week 16).

use std::fs;
use std::path::{Path, PathBuf};

use crate::compaction::{self, L0_COMPACTION_TRIGGER};
use crate::engine::StorageEngine;
use crate::error::StorageError;
use crate::memtable::{MemTable, DEFAULT_MAX_MEM_BYTES};
use crate::sstable::{SstReader, SstWriter};
use crate::wal::{LogEntry, WalSegmentManager};

/// Configuration for [`LsmTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsmConfig {
    /// Rotate MemTable when active table exceeds this many bytes.
    pub max_mem_bytes: usize,
    /// Start L0→L1 compaction at this many L0 files.
    pub l0_compaction_trigger: usize,
}

impl Default for LsmConfig {
    fn default() -> Self {
        Self {
            max_mem_bytes: DEFAULT_MAX_MEM_BYTES,
            l0_compaction_trigger: L0_COMPACTION_TRIGGER,
        }
    }
}

/// Complete LSM storage engine: WAL + MemTable + SSTables + compaction.
pub struct LsmTree {
    dir: PathBuf,
    wal: WalSegmentManager,
    active: MemTable,
    level0: Vec<PathBuf>,
    level1: Vec<PathBuf>,
    config: LsmConfig,
    flushed_wal_segment: u64,
}

impl LsmTree {
    /// Open or create an LSM tree at `data_dir`, replaying WAL on startup.
    pub fn open(data_dir: impl AsRef<Path>, config: LsmConfig) -> Result<Self, StorageError> {
        let dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        fs::create_dir_all(dir.join("sst"))?;

        let mut active = MemTable::new();
        WalSegmentManager::replay_all(&dir, &mut active)?;

        let level0 = compaction::list_sst_level(&dir.join("sst"), 0)?;
        let level1 = compaction::list_sst_level(&dir.join("sst"), 1)?;
        let wal = WalSegmentManager::open(&dir)?;

        Ok(Self {
            dir,
            wal,
            active,
            level0,
            level1,
            config,
            flushed_wal_segment: 0,
        })
    }

    /// Open with default [`LsmConfig`].
    pub fn open_default(data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open(data_dir, LsmConfig::default())
    }

    /// Data directory path.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    /// Insert or overwrite a key (WAL → MemTable → maybe flush).
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.wal
            .append(&LogEntry::put(key.to_vec(), value.to_vec()))?;
        self.active.put(key, value)?;
        self.maybe_flush_and_compact()?;
        Ok(())
    }

    /// Delete a key.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, StorageError> {
        self.wal.append(&LogEntry::delete(key.to_vec()))?;
        let removed = self.active.delete(key)?;
        self.maybe_flush_and_compact()?;
        Ok(removed)
    }

    /// Read path: active MemTable → L0 (newest first) → L1.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(v) = self.active.get(key)? {
            return Ok(Some(v));
        }
        for path in self.level0.iter().rev() {
            if let Some(v) = SstReader::open(path)?.get(key)? {
                return Ok(Some(v));
            }
        }
        for path in self.level1.iter().rev() {
            if let Some(v) = SstReader::open(path)?.get(key)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Number of L0 SSTable files.
    #[must_use]
    pub fn l0_count(&self) -> usize {
        self.level0.len()
    }

    fn maybe_flush_and_compact(&mut self) -> Result<(), StorageError> {
        if self.active.approx_bytes() < self.config.max_mem_bytes {
            return Ok(());
        }
        self.flush_active()?;
        if self.level0.len() >= self.config.l0_compaction_trigger {
            self.run_compaction()?;
        }
        Ok(())
    }

    fn flush_active(&mut self) -> Result<(), StorageError> {
        let frozen = core::mem::take(&mut self.active);
        if frozen.is_empty() {
            return Ok(());
        }

        let path = compaction::alloc_sst_path(&self.dir.join("sst"), 0)?;
        SstWriter::write_from_memtable(&path, &frozen)?;
        self.level0.push(path);

        let old_wal = self.wal.rotate()?;
        for seg in self.flushed_wal_segment..=old_wal {
            self.wal.delete_segment(seg)?;
        }
        self.flushed_wal_segment = self.wal.active_id();
        Ok(())
    }

    fn run_compaction(&mut self) -> Result<(), StorageError> {
        let l0 = self.level0.clone();
        let l1_path = compaction::compact_level0_to_l1(&self.dir, &l0)?;
        self.level0.clear();
        self.level1.push(l1_path);
        Ok(())
    }
}

impl StorageEngine for LsmTree {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        LsmTree::get(self, key)
    }

    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        LsmTree::put(self, key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<bool, Self::Error> {
        LsmTree::delete(self, key)
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        let mut merged = std::collections::BTreeMap::new();
        for path in self.level1.iter().chain(self.level0.iter()) {
            if let Ok(reader) = SstReader::open(path) {
                if let Ok(iter) = reader.scan() {
                    for item in iter.flatten() {
                        merged.insert(item.0, item.1);
                    }
                }
            }
        }
        for (k, v) in self.active.iter() {
            merged.insert(k, v);
        }
        merged.into_iter()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-lsm-{prefix}-{nanos}"))
    }

    #[test]
    fn crash_recovery_replays_wal_on_reopen() {
        let dir = temp_dir("crash");
        {
            let mut tree = LsmTree::open(
                &dir,
                LsmConfig {
                    max_mem_bytes: 1_000_000,
                    l0_compaction_trigger: 99,
                },
            )
            .unwrap();
            tree.put(b"persist", b"yes").unwrap();
        }
        let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        assert_eq!(tree.get(b"persist").unwrap(), Some(b"yes".to_vec()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn flush_creates_l0_sstable() {
        let dir = temp_dir("flush");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 48,
                l0_compaction_trigger: 99,
            },
        )
        .unwrap();
        tree.put(
            b"0123456789012345678901234567890123456789012345678901234567890",
            b"x",
        )
        .unwrap();
        assert!(tree.l0_count() >= 1);
        assert_eq!(
            tree.get(b"0123456789012345678901234567890123456789012345678901234567890")
                .unwrap(),
            Some(b"x".to_vec())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ten_k_puts_readable() {
        let dir = temp_dir("10k");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 4096,
                l0_compaction_trigger: 4,
            },
        )
        .unwrap();

        for i in 0..10_000u32 {
            let k = format!("row:{i:05}");
            tree.put(k.as_bytes(), b"data").unwrap();
        }
        for i in (0..10_000u32).step_by(10) {
            let k = format!("row:{i:05}");
            assert_eq!(tree.get(k.as_bytes()).unwrap(), Some(b"data".to_vec()));
        }
        let _ = fs::remove_dir_all(dir);
    }
}
