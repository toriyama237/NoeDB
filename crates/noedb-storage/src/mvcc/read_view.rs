//! Snapshot read view (Phase 2 Week 9 — Snapshot Isolation).

use std::collections::BTreeSet;

use super::{CommitTs, TxnId, Version};

/// Snapshot taken at transaction `BEGIN`.
#[derive(Debug, Clone)]
pub struct ReadView {
    /// Snapshot timestamp — versions with `commit_ts <= snapshot_ts` may be visible.
    pub snapshot_ts: CommitTs,
    /// Transactions still active when snapshot was taken (their commits are invisible).
    pub active_txns: BTreeSet<TxnId>,
    /// Owning transaction (sees its own uncommitted intents).
    pub txn_id: TxnId,
}

impl ReadView {
    /// Create a snapshot at `snapshot_ts`.
    #[must_use]
    pub fn new(txn_id: TxnId, snapshot_ts: CommitTs, active_txns: BTreeSet<TxnId>) -> Self {
        Self {
            snapshot_ts,
            active_txns,
            txn_id,
        }
    }

    /// Whether `version` is visible to this reader.
    #[must_use]
    pub fn is_visible(&self, version: &Version, writer_txn: Option<TxnId>) -> bool {
        if version.commit_ts == 0 {
            return writer_txn == Some(self.txn_id);
        }
        if version.commit_ts > self.snapshot_ts {
            return false;
        }
        if let Some(w) = writer_txn {
            if self.active_txns.contains(&w) && w != self.txn_id {
                return false;
            }
        }
        true
    }
}
