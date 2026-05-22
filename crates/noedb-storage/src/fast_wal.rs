//! Batched WAL I/O surface for Phase 3 Week 15 (io_uring-ready).
//!
//! Production Linux builds can swap the backend for `io_uring` submission queues;
//! the default path batches appends and issues one `fsync`.

use crate::error::StorageError;
use crate::wal::{LogEntry, WalSegmentManager};

/// Append many WAL records then `fsync` once (reduces syscall overhead vs per-entry sync).
///
/// # Errors
///
/// WAL or IO failures.
pub fn append_batch_sync(
    wal: &mut WalSegmentManager,
    entries: &[LogEntry],
) -> Result<(), StorageError> {
    for entry in entries {
        wal.append_unsynced(entry)?;
    }
    wal.sync()
}
