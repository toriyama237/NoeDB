//! Durable storage: WAL-first writes + MemTable with flush rotation.

use std::path::Path;

use crate::engine::StorageEngine;
use crate::error::StorageError;
use crate::memtable::{MemTable, DEFAULT_MAX_MEM_BYTES};
use crate::wal::{LogEntry, OpType, Wal};

/// A durable key-value store: every mutation is appended to the WAL and
/// [`sync_all`](Wal::sync) before it reaches the active [`MemTable`].
///
/// When the active table exceeds [`max_mem_bytes`](Self::max_mem_bytes), it
/// is frozen and swapped atomically for a fresh table (SSTable flush lands
/// in Week 12).
pub struct DurableStore {
    wal: Wal,
    active: MemTable,
    immutable: Vec<MemTable>,
    max_mem_bytes: usize,
}

impl DurableStore {
    /// Open or create a store at `wal_path`.
    pub fn open(wal_path: impl AsRef<Path>, max_mem_bytes: usize) -> Result<Self, StorageError> {
        let wal = Wal::open(wal_path)?;
        Ok(Self {
            wal,
            active: MemTable::new(),
            immutable: Vec::new(),
            max_mem_bytes,
        })
    }

    /// Open with [`DEFAULT_MAX_MEM_BYTES`].
    pub fn open_default(wal_path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open(wal_path, DEFAULT_MAX_MEM_BYTES)
    }

    /// Configured MemTable rotation threshold.
    #[must_use]
    pub const fn max_mem_bytes(&self) -> usize {
        self.max_mem_bytes
    }

    /// Number of frozen MemTables waiting for SSTable flush.
    #[must_use]
    pub fn immutable_count(&self) -> usize {
        self.immutable.len()
    }

    /// Borrow the active MemTable (read-only).
    #[must_use]
    pub fn active(&self) -> &MemTable {
        &self.active
    }

    /// Borrow frozen MemTables (oldest first).
    #[must_use]
    pub fn immutable(&self) -> &[MemTable] {
        &self.immutable
    }

    /// WAL file path.
    #[must_use]
    pub fn wal_path(&self) -> &Path {
        self.wal.path()
    }

    /// Append to WAL, sync, apply to MemTable, maybe rotate.
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.append(OpType::Put, key, Some(value))?;
        self.active.put(key, value)?;
        self.maybe_rotate();
        Ok(())
    }

    /// Append delete to WAL, sync, apply to MemTable, maybe rotate.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, StorageError> {
        self.append(OpType::Delete, key, None)?;
        let removed = self.active.delete(key)?;
        self.maybe_rotate();
        Ok(removed)
    }

    /// Look up `key` in active then immutable tables (newest first).
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(v) = self.active.get(key)? {
            return Ok(Some(v));
        }
        for table in self.immutable.iter().rev() {
            if let Some(v) = table.get(key)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    fn append(&mut self, op: OpType, key: &[u8], value: Option<&[u8]>) -> Result<(), StorageError> {
        let entry = match op {
            OpType::Put => {
                let val = value.ok_or(StorageError::invalid_input("put requires value"))?;
                LogEntry::put(key.to_vec(), val.to_vec())
            }
            OpType::Delete => LogEntry::delete(key.to_vec()),
        };
        self.wal.append(&entry)
    }

    fn maybe_rotate(&mut self) {
        if self.active.approx_bytes() >= self.max_mem_bytes {
            let frozen = core::mem::take(&mut self.active);
            self.immutable.push(frozen);
            self.active = MemTable::new();
        }
    }
}

impl StorageEngine for DurableStore {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        DurableStore::get(self, key)
    }

    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        DurableStore::put(self, key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<bool, Self::Error> {
        DurableStore::delete(self, key)
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        let mut merged = std::collections::BTreeMap::new();
        for table in self.immutable.iter().chain(std::iter::once(&self.active)) {
            for (k, v) in table.iter() {
                merged.insert(k, v);
            }
        }
        merged.into_iter()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_wal() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-store-{nanos}.log"))
    }

    #[test]
    fn wal_persists_before_memtable_read() {
        let path = temp_wal();
        {
            let mut store = DurableStore::open(&path, 1024 * 1024).unwrap();
            store.put(b"k", b"v").unwrap();
        }
        let entries = Wal::read_all(&path).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].op, OpType::Put);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rotate_when_threshold_exceeded() {
        let path = temp_wal();
        let mut store = DurableStore::open(&path, 32).unwrap();
        store.put(b"0123456789012345678901234567890", b"x").unwrap();
        assert_eq!(store.immutable_count(), 1);
        assert!(store.active().is_empty());
        assert_eq!(
            store.get(b"0123456789012345678901234567890").unwrap(),
            Some(b"x".to_vec())
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn replay_wal_rebuilds_memtable() {
        let path = temp_wal();
        {
            let mut store = DurableStore::open(&path, 4096).unwrap();
            store.put(b"a", b"1").unwrap();
            store.put(b"b", b"2").unwrap();
            store.delete(b"a").unwrap();
        }
        let mut rebuilt = MemTable::new();
        crate::wal::replay_into_memtable(&path, &mut rebuilt).unwrap();
        assert_eq!(rebuilt.get(b"b").unwrap(), Some(b"2".to_vec()));
        assert_eq!(rebuilt.get(b"a").unwrap(), None);
        let _ = std::fs::remove_file(path);
    }
}
