//! Append-only write-ahead log with `fsync` durability.

mod entry;
mod segments;

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub use entry::{LogEntry, OpType, WAL_MAGIC, WAL_VERSION};

use crate::error::StorageError;

const HEADER_LEN: u64 = 6;

/// Append-only WAL backed by a regular file.
pub struct Wal {
    path: PathBuf,
    file: File,
}

impl Wal {
    /// Create or open a WAL at `path`, writing the file header when new.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(StorageError::from)?;

        let len = file.metadata().map_err(StorageError::from)?.len();
        if len == 0 {
            Self::write_header(&mut file)?;
            file.sync_all().map_err(StorageError::from)?;
        } else if len < HEADER_LEN {
            return Err(StorageError::corrupt_wal("truncated WAL header"));
        } else {
            Self::validate_header(&mut file)?;
        }

        file.seek(SeekFrom::End(0)).map_err(StorageError::from)?;
        Ok(Self { path, file })
    }

    /// Path to the underlying WAL file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a record. When `sync` is false, call [`Self::sync`] before crash-safe durability.
    pub fn append(&mut self, entry: &LogEntry, sync: bool) -> Result<(), StorageError> {
        let bytes = entry.encode()?;
        self.file.write_all(&bytes).map_err(StorageError::from)?;
        if sync {
            self.sync()?;
        }
        Ok(())
    }

    /// Append and [`sync_all`](Self::sync) before returning (legacy durable path).
    pub fn append_durable(&mut self, entry: &LogEntry) -> Result<(), StorageError> {
        self.append(entry, true)
    }

    /// Durably flush buffered WAL data to disk.
    pub fn sync(&mut self) -> Result<(), StorageError> {
        self.file.sync_all().map_err(StorageError::from)
    }

    /// Read every record in file order.
    pub fn read_all(path: impl AsRef<Path>) -> Result<Vec<LogEntry>, StorageError> {
        let path = path.as_ref();
        let mut file = File::open(path).map_err(StorageError::from)?;
        Self::validate_header(&mut file)?;

        let mut buf = Vec::new();
        file.read_to_end(&mut buf).map_err(StorageError::from)?;

        let mut entries = Vec::new();
        let mut off = 0usize;
        while off < buf.len() {
            let (entry, n) = LogEntry::decode(&buf[off..])?;
            entries.push(entry);
            off += n;
        }
        Ok(entries)
    }

    fn write_header(file: &mut File) -> Result<(), StorageError> {
        file.write_all(&WAL_MAGIC).map_err(StorageError::from)?;
        file.write_all(&WAL_VERSION.to_le_bytes())
            .map_err(StorageError::from)
    }

    fn validate_header(file: &mut File) -> Result<(), StorageError> {
        file.seek(SeekFrom::Start(0)).map_err(StorageError::from)?;
        let mut magic = [0u8; 4];
        file.read_exact(&mut magic).map_err(StorageError::from)?;
        if magic != WAL_MAGIC {
            return Err(StorageError::corrupt_wal("bad WAL magic"));
        }
        let mut ver = [0u8; 2];
        file.read_exact(&mut ver).map_err(StorageError::from)?;
        if u16::from_le_bytes(ver) != WAL_VERSION {
            return Err(StorageError::corrupt_wal("unsupported WAL version"));
        }
        Ok(())
    }
}

pub use segments::{replay_wal_dir, WalSegmentManager, WalSyncMode};

/// Replay a single WAL file into `table`.
pub fn replay_into_memtable(
    path: impl AsRef<Path>,
    table: &mut crate::MemTable,
) -> Result<usize, StorageError> {
    let entries = Wal::read_all(path)?;
    let count = entries.len();
    for entry in entries {
        match entry.op {
            OpType::Put => table.put(&entry.key, &entry.value)?,
            OpType::Delete => {
                table.delete(&entry.key)?;
            }
        }
    }
    Ok(count)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_wal() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-wal-{nanos}.log"))
    }

    #[test]
    fn append_and_read_back_identical_entries() {
        let path = temp_wal();
        let entries = vec![
            LogEntry::put(b"a".to_vec(), b"1".to_vec()),
            LogEntry::put(b"b".to_vec(), b"two".to_vec()),
            LogEntry::delete(b"a".to_vec()),
        ];

        {
            let mut wal = Wal::open(&path).unwrap();
            for e in &entries {
                wal.append_durable(e).unwrap();
            }
        }

        let read = Wal::read_all(&path).unwrap();
        assert_eq!(read, entries);
        let _ = std::fs::remove_file(path);
    }
}
