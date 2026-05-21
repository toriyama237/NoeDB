//! Serializable Snapshot Isolation — write/read dependency detection (Week 10).

use std::collections::{BTreeMap, BTreeSet};

use crate::{Transaction, TxnId};

/// SSI decision for a commit attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SsiDecision {
    /// Safe to commit.
    Allow,
    /// Abort this txn (youngest victim).
    Abort(&'static str),
}

/// Tracks rw-dependencies between concurrent transactions.
#[derive(Debug, Default)]
pub struct SsiChecker {
    /// `reader -> writers` that wrote keys the reader later read.
    rw_edges: BTreeMap<TxnId, BTreeSet<TxnId>>,
    /// Keys written by txn (for conflict checks).
    write_keys: BTreeMap<TxnId, BTreeSet<Vec<u8>>>,
}

impl SsiChecker {
    /// Register committed write keys for visibility conflict detection.
    pub fn register_commit(&mut self, txn_id: TxnId, keys: BTreeSet<Vec<u8>>) {
        self.write_keys.insert(txn_id, keys);
    }

    /// Record that `reader` read a key previously written by `writer` while writer was in-flight.
    pub fn note_rw_dependency(&mut self, reader: TxnId, writer: TxnId) {
        if reader != writer {
            self.rw_edges.entry(reader).or_default().insert(writer);
        }
    }

    /// Check for dangerous structure before commit (outgoing rw + incoming wr cycle).
    pub fn check_commit(&self, txn: &Transaction) -> SsiDecision {
        let my_writes: BTreeSet<_> = txn.write_set.keys().cloned().collect();
        for (&other_id, keys) in &self.write_keys {
            if other_id == txn.id {
                continue;
            }
            for k in keys {
                if txn.read_set.contains(k) && my_writes.contains(k) {
                    return SsiDecision::Abort("write-read conflict on overlapping key");
                }
            }
        }

        if let Some(out) = self.rw_edges.get(&txn.id) {
            for &pred in out {
                if self.rw_edges.get(&pred).is_some_and(|s| s.contains(&txn.id)) {
                    return SsiDecision::Abort("rw-cycle detected (SSI)");
                }
            }
        }
        SsiDecision::Allow
    }

    /// Drop state for finished txn.
    pub fn purge(&mut self, txn_id: TxnId) {
        self.rw_edges.remove(&txn_id);
        self.write_keys.remove(&txn_id);
        self.rw_edges.retain(|_, writers| {
            writers.remove(&txn_id);
            !writers.is_empty()
        });
    }
}
