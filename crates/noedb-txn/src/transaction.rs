//! In-flight transaction state.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use noedb_storage::mvcc::CommitTs;

use crate::TxnId;

/// Pending write operation in the write set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteOp {
    /// Upsert cell value.
    Put(Vec<u8>),
    /// Delete tombstone.
    Delete,
}

/// Lifecycle of a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnState {
    /// Running — reads/writes allowed.
    Active,
    /// Successfully committed.
    Committed,
    /// Rolled back or aborted.
    Aborted,
}

/// One SQL transaction (`BEGIN` … `COMMIT` / `ROLLBACK`).
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Unique id.
    pub id: TxnId,
    /// Snapshot timestamp assigned at `BEGIN`.
    pub start_ts: CommitTs,
    /// Keys read (for SSI / phantom detection).
    pub read_set: BTreeSet<Vec<u8>>,
    /// Buffered writes until commit.
    pub write_set: BTreeMap<Vec<u8>, WriteOp>,
    /// Current state.
    pub state: TxnState,
    /// When the txn started (deadlock timeout).
    pub started_at: Instant,
    /// Keys this txn is waiting to lock.
    pub waiting_on: Option<Vec<u8>>,
}

impl Transaction {
    /// New active transaction.
    #[must_use]
    pub fn new(id: TxnId, start_ts: CommitTs) -> Self {
        Self {
            id,
            start_ts,
            read_set: BTreeSet::new(),
            write_set: BTreeMap::new(),
            state: TxnState::Active,
            started_at: Instant::now(),
            waiting_on: None,
        }
    }

    /// Record a read for conflict tracking.
    pub fn record_read(&mut self, key: Vec<u8>) {
        self.read_set.insert(key);
    }

    /// Stage a put in the write set (read-your-writes).
    pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.write_set.insert(key, WriteOp::Put(value));
    }

    /// Stage a delete.
    pub fn delete(&mut self, key: Vec<u8>) {
        self.write_set.insert(key, WriteOp::Delete);
    }

    /// Read-your-writes: pending value if present.
    #[must_use]
    pub fn read_own(&self, key: &[u8]) -> Option<Option<&[u8]>> {
        match self.write_set.get(key) {
            None => None,
            Some(WriteOp::Put(v)) => Some(Some(v.as_slice())),
            Some(WriteOp::Delete) => Some(None),
        }
    }
}
