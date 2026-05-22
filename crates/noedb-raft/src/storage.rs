//! Durable storage trait + in-memory / file backends (Week 34).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::StorageError;
use crate::log::{LogEntry, RaftLog};
use crate::security::{checksum, verify_checksum};
use crate::state::HardState;
use crate::types::{LogIndex, Term};

/// Snapshot metadata + opaque state-machine bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Last index included in snapshot.
    pub index: LogIndex,
    /// Term of that index.
    pub term: Term,
    /// Serialized application state.
    pub data: Vec<u8>,
}

/// Persistent Raft store (etcd/RocksDB-style boundary).
pub trait RaftStorage: Send {
    /// Load hard state; default if missing.
    ///
    /// # Errors
    ///
    /// Storage or corruption errors.
    fn load_hard_state(&self) -> Result<HardState, StorageError>;

    /// Persist hard state (must fsync before RPC reply when required).
    ///
    /// # Errors
    ///
    /// Storage errors.
    fn save_hard_state(&mut self, hs: &HardState) -> Result<(), StorageError>;

    /// Load full log.
    ///
    /// # Errors
    ///
    /// Storage errors.
    fn load_log(&self) -> Result<RaftLog, StorageError>;

    /// Append entries starting at `from_index` (truncates tail if needed).
    ///
    /// # Errors
    ///
    /// Storage errors.
    fn append(&mut self, from_index: LogIndex, entries: &[LogEntry]) -> Result<(), StorageError>;

    /// Load latest snapshot if any.
    ///
    /// # Errors
    ///
    /// Storage errors.
    fn load_snapshot(&self) -> Result<Option<Snapshot>, StorageError>;

    /// Store snapshot and compact log prefix.
    ///
    /// # Errors
    ///
    /// Storage errors.
    fn save_snapshot(&mut self, snap: &Snapshot) -> Result<(), StorageError>;

    /// Flush durable buffers (group-commit hook).
    ///
    /// # Errors
    ///
    /// Storage errors.
    fn sync(&mut self) -> Result<(), StorageError> {
        Ok(())
    }
}

/// In-memory store for tests and benchmarks.
#[derive(Debug, Default)]
pub struct MemStorage {
    hard: HardState,
    log: RaftLog,
    snapshot: Option<Snapshot>,
}

impl MemStorage {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            hard: HardState::default(),
            log: RaftLog::new(),
            snapshot: None,
        }
    }
}

impl RaftStorage for MemStorage {
    fn load_hard_state(&self) -> Result<HardState, StorageError> {
        Ok(self.hard.clone())
    }

    fn save_hard_state(&mut self, hs: &HardState) -> Result<(), StorageError> {
        self.hard = hs.clone();
        Ok(())
    }

    fn load_log(&self) -> Result<RaftLog, StorageError> {
        Ok(self.log.clone())
    }

    fn append(&mut self, from_index: LogIndex, entries: &[LogEntry]) -> Result<(), StorageError> {
        self.log.truncate_from(from_index.prev());
        self.log.append(entries);
        Ok(())
    }

    fn load_snapshot(&self) -> Result<Option<Snapshot>, StorageError> {
        Ok(self.snapshot.clone())
    }

    fn save_snapshot(&mut self, snap: &Snapshot) -> Result<(), StorageError> {
        self.snapshot = Some(snap.clone());
        self.log.restore(snap.index, snap.term, Vec::new());
        Ok(())
    }
}

/// File-backed store with CRC-protected records.
pub struct FileStorage {
    dir: std::path::PathBuf,
    hard: HardState,
    log: RaftLog,
    snapshot: Option<Snapshot>,
    dirty: bool,
}

impl FileStorage {
    /// Open or create storage under `dir`.
    ///
    /// # Errors
    ///
    /// I/O or corruption errors.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        let dir = dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&dir).map_err(StorageError::io)?;
        let mut s = Self {
            dir,
            hard: HardState::default(),
            log: RaftLog::new(),
            snapshot: None,
            dirty: false,
        };
        s.reload()?;
        Ok(s)
    }

    fn hard_path(&self) -> std::path::PathBuf {
        self.dir.join("hard_state.bin")
    }

    fn log_path(&self) -> std::path::PathBuf {
        self.dir.join("log.bin")
    }

    fn snap_path(&self) -> std::path::PathBuf {
        self.dir.join("snapshot.bin")
    }

    fn reload(&mut self) -> Result<(), StorageError> {
        if self.hard_path().exists() {
            let bytes = std::fs::read(self.hard_path()).map_err(StorageError::io)?;
            let (payload, crc) = decode_record(&bytes)?;
            verify_checksum(&payload, crc).map_err(|e| StorageError::Corrupt(e.to_string()))?;
            self.hard = bincode::deserialize(&payload)
                .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        }
        if self.log_path().exists() {
            let bytes = std::fs::read(self.log_path()).map_err(StorageError::io)?;
            let (payload, crc) = decode_record(&bytes)?;
            verify_checksum(&payload, crc).map_err(|e| StorageError::Corrupt(e.to_string()))?;
            self.log = bincode::deserialize(&payload)
                .map_err(|e| StorageError::Corrupt(e.to_string()))?;
        }
        if self.snap_path().exists() {
            let bytes = std::fs::read(self.snap_path()).map_err(StorageError::io)?;
            let (payload, crc) = decode_record(&bytes)?;
            verify_checksum(&payload, crc).map_err(|e| StorageError::Corrupt(e.to_string()))?;
            self.snapshot = Some(
                bincode::deserialize(&payload).map_err(|e| StorageError::Corrupt(e.to_string()))?,
            );
        }
        Ok(())
    }

    fn write_record(path: &Path, value: &[u8]) -> Result<(), StorageError> {
        let crc = checksum(value);
        let mut out = Vec::with_capacity(4 + value.len());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(value);
        std::fs::write(path, &out).map_err(StorageError::io)?;
        Ok(())
    }
}

impl RaftStorage for FileStorage {
    fn load_hard_state(&self) -> Result<HardState, StorageError> {
        Ok(self.hard.clone())
    }

    fn save_hard_state(&mut self, hs: &HardState) -> Result<(), StorageError> {
        self.hard = hs.clone();
        self.dirty = true;
        Ok(())
    }

    fn load_log(&self) -> Result<RaftLog, StorageError> {
        Ok(self.log.clone())
    }

    fn append(&mut self, from_index: LogIndex, entries: &[LogEntry]) -> Result<(), StorageError> {
        self.log.truncate_from(from_index.prev());
        self.log.append(entries);
        self.dirty = true;
        Ok(())
    }

    fn load_snapshot(&self) -> Result<Option<Snapshot>, StorageError> {
        Ok(self.snapshot.clone())
    }

    fn save_snapshot(&mut self, snap: &Snapshot) -> Result<(), StorageError> {
        self.snapshot = Some(snap.clone());
        self.log.restore(snap.index, snap.term, Vec::new());
        self.dirty = true;
        Ok(())
    }

    fn sync(&mut self) -> Result<(), StorageError> {
        if !self.dirty {
            return Ok(());
        }
        let hard_bytes =
            bincode::serialize(&self.hard).map_err(|e| StorageError::Corrupt(e.to_string()))?;
        Self::write_record(&self.hard_path(), &hard_bytes)?;
        let log_bytes =
            bincode::serialize(&self.log.entries()).map_err(|e| StorageError::Corrupt(e.to_string()))?;
        Self::write_record(&self.log_path(), &log_bytes)?;
        if let Some(ref snap) = self.snapshot {
            let snap_bytes =
                bincode::serialize(snap).map_err(|e| StorageError::Corrupt(e.to_string()))?;
            Self::write_record(&self.snap_path(), &snap_bytes)?;
        }
        self.dirty = false;
        Ok(())
    }
}

fn decode_record(bytes: &[u8]) -> Result<(Vec<u8>, u32), StorageError> {
    if bytes.len() < 4 {
        return Err(StorageError::Corrupt("record too short".into()));
    }
    let crc = u32::from_le_bytes(bytes[..4].try_into().map_err(|_| StorageError::Corrupt("crc".into()))?);
    Ok((bytes[4..].to_vec(), crc))
}
