//! One stored version of a user key.

use super::CommitTs;

/// A single MVCC version (LSM internal record payload).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Commit timestamp (0 = uncommitted intent, visible only to owning txn).
    pub commit_ts: CommitTs,
    /// Cell bytes (empty when [`Self::deleted`]).
    pub value: Vec<u8>,
    /// Tombstone marker (delete version).
    pub deleted: bool,
}

impl Version {
    /// New put version at `commit_ts`.
    #[must_use]
    pub fn put(commit_ts: CommitTs, value: Vec<u8>) -> Self {
        Self {
            commit_ts,
            value,
            deleted: false,
        }
    }

    /// Tombstone at `commit_ts`.
    #[must_use]
    pub fn tombstone(commit_ts: CommitTs) -> Self {
        Self {
            commit_ts,
            value: Vec::new(),
            deleted: true,
        }
    }

    /// Uncommitted write intent (visible to owning txn only).
    #[must_use]
    pub fn intent(value: Vec<u8>) -> Self {
        Self {
            commit_ts: 0,
            value,
            deleted: false,
        }
    }

    /// Uncommitted delete intent.
    #[must_use]
    pub fn delete_intent() -> Self {
        Self {
            commit_ts: 0,
            value: Vec::new(),
            deleted: true,
        }
    }
}
