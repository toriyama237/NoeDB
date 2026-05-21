//! WAL segment rotation and multi-file replay (Week 11).

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use super::{LogEntry, Wal};
use crate::error::StorageError;
use crate::memtable::MemTable;

const CURRENT_FILE: &str = "CURRENT";
const SEGMENT_EXT: &str = "wal";

/// Manages numbered WAL segments under `wal/`.
pub struct WalSegmentManager {
    dir: PathBuf,
    active_id: u64,
    active: Wal,
}

impl WalSegmentManager {
    /// Open or create WAL segments under `data_dir/wal/`.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let dir = data_dir.as_ref().join("wal");
        fs::create_dir_all(&dir)?;

        let active_id = Self::read_current(&dir).unwrap_or(1);
        let path = Self::segment_path(&dir, active_id);
        let active = Wal::open(path)?;

        Ok(Self {
            dir,
            active_id,
            active,
        })
    }

    /// Active segment id.
    #[must_use]
    pub const fn active_id(&self) -> u64 {
        self.active_id
    }

    /// Append to the active segment (synced).
    pub fn append(&mut self, entry: &LogEntry) -> Result<(), StorageError> {
        self.active.append(entry)
    }

    /// Rotate to a fresh segment after MemTable flush (Week 11).
    pub fn rotate(&mut self) -> Result<u64, StorageError> {
        let old_id = self.active_id;
        self.active_id += 1;
        let path = Self::segment_path(&self.dir, self.active_id);
        self.active = Wal::open(path)?;
        Self::write_current(&self.dir, self.active_id)?;
        Ok(old_id)
    }

    /// Delete a segment file after its data has been flushed to SSTable.
    pub fn delete_segment(&self, id: u64) -> Result<(), StorageError> {
        let path = Self::segment_path(&self.dir, id);
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Replay every segment in order into `table`.
    pub fn replay_all(dir: impl AsRef<Path>, table: &mut MemTable) -> Result<usize, StorageError> {
        let dir = dir.as_ref().join("wal");
        if !dir.exists() {
            return Ok(0);
        }

        let mut ids = Self::list_segment_ids(&dir)?;
        ids.sort_unstable();

        let mut total = 0usize;
        for id in ids {
            let path = Self::segment_path(&dir, id);
            total += super::replay_into_memtable(&path, table)?;
        }
        Ok(total)
    }

    fn segment_path(dir: &Path, id: u64) -> PathBuf {
        dir.join(format!("{id:020}.{SEGMENT_EXT}"))
    }

    fn list_segment_ids(dir: &Path) -> Result<Vec<u64>, StorageError> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if let Some(stem) = name.strip_suffix(&format!(".{SEGMENT_EXT}")) {
                if let Ok(id) = stem.parse::<u64>() {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    fn read_current(dir: &Path) -> Result<u64, StorageError> {
        let path = dir.join(CURRENT_FILE);
        let mut file = File::open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        buf.trim()
            .parse::<u64>()
            .map_err(|_| StorageError::corrupt_wal("invalid CURRENT file"))
    }

    fn write_current(dir: &Path, id: u64) -> Result<(), StorageError> {
        let path = dir.join(CURRENT_FILE);
        let mut file = File::create(path)?;
        writeln!(file, "{id}")?;
        file.sync_all()?;
        Ok(())
    }
}

/// Replay all WAL segments under a data directory (public helper).
pub fn replay_wal_dir(
    data_dir: impl AsRef<Path>,
    table: &mut MemTable,
) -> Result<usize, StorageError> {
    WalSegmentManager::replay_all(data_dir, table)
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
        std::env::temp_dir().join(format!("noedb-{prefix}-{nanos}"))
    }

    #[test]
    fn rotate_creates_new_segment() {
        let dir = temp_dir("wal-rot");
        let mut mgr = WalSegmentManager::open(&dir).unwrap();
        mgr.append(&LogEntry::put(b"a".to_vec(), b"1".to_vec()))
            .unwrap();
        let old = mgr.rotate().unwrap();
        assert_eq!(old, 1);
        assert_eq!(mgr.active_id(), 2);
        mgr.append(&LogEntry::put(b"b".to_vec(), b"2".to_vec()))
            .unwrap();

        let mut table = MemTable::new();
        let n = WalSegmentManager::replay_all(&dir, &mut table).unwrap();
        assert_eq!(n, 2);
        assert_eq!(table.get(b"a").unwrap(), Some(b"1".to_vec()));
        assert_eq!(table.get(b"b").unwrap(), Some(b"2".to_vec()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_segment_after_flush() {
        let dir = temp_dir("wal-del");
        let mut mgr = WalSegmentManager::open(&dir).unwrap();
        mgr.append(&LogEntry::put(b"x".to_vec(), b"y".to_vec()))
            .unwrap();
        let old = mgr.rotate().unwrap();
        mgr.delete_segment(old).unwrap();
        let ids = WalSegmentManager::list_segment_ids(&dir.join("wal")).unwrap();
        assert!(!ids.contains(&old));
        let _ = fs::remove_dir_all(dir);
    }
}
