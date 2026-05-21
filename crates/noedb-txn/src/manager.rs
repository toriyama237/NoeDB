//! Central transaction coordinator.

use std::collections::{BTreeMap, BTreeSet};

use noedb_storage::mvcc::{gc_versions, CommitTs, MvccMemTable, ReadView, TimestampOracle};
use parking_lot::Mutex;

use crate::deadlock::DeadlockGuard;
use crate::error::TxnError;
use crate::ssi::{SsiChecker, SsiDecision};
use crate::transaction::{Transaction, TxnState, WriteOp};
use crate::TxnId;

/// Result of a successful commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    /// Assigned commit timestamp.
    pub commit_ts: CommitTs,
    /// Keys written.
    pub keys_written: usize,
}

/// Process-wide transaction manager (single-node).
pub struct TxnManager {
    oracle: TimestampOracle,
    mvcc: Mutex<MvccMemTable>,
    next_id: Mutex<TxnId>,
    active: Mutex<BTreeMap<TxnId, Transaction>>,
    ssi: Mutex<SsiChecker>,
    deadlock: Mutex<DeadlockGuard>,
    /// Per-session active txn (session_id → txn_id).
    sessions: Mutex<BTreeMap<u64, TxnId>>,
}

impl Default for TxnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TxnManager {
    /// Fresh manager with empty MVCC memtable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            oracle: TimestampOracle::new(),
            mvcc: Mutex::new(MvccMemTable::new()),
            next_id: Mutex::new(1),
            active: Mutex::new(BTreeMap::new()),
            ssi: Mutex::new(SsiChecker::default()),
            deadlock: Mutex::new(DeadlockGuard::new()),
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    /// Shared MVCC table (tests / GC).
    pub fn mvcc(&self) -> parking_lot::MutexGuard<'_, MvccMemTable> {
        self.mvcc.lock()
    }

    /// Timestamp oracle reference.
    #[must_use]
    pub fn oracle(&self) -> &TimestampOracle {
        &self.oracle
    }

    /// `BEGIN` — assign `start_ts` atomically, capture active txn snapshot.
    pub fn begin(&self, session_id: u64) -> Result<TxnId, TxnError> {
        let mut sessions = self.sessions.lock();
        if sessions.contains_key(&session_id) {
            return Err(TxnError::TxnAlreadyActive);
        }

        let start_ts = self.oracle.next();
        let mut next = self.next_id.lock();
        let id = *next;
        *next = next.saturating_add(1);

        let txn = Transaction::new(id, start_ts);
        self.active.lock().insert(id, txn);
        sessions.insert(session_id, id);
        Ok(id)
    }

    /// Build read view for session's active txn.
    pub fn read_view(&self, session_id: u64) -> Result<ReadView, TxnError> {
        let sessions = self.sessions.lock();
        let txn_id = *sessions.get(&session_id).ok_or(TxnError::NoActiveTxn)?;
        let active = self.active.lock();
        let txn = active.get(&txn_id).ok_or(TxnError::NoActiveTxn)?;
        let active_ids: BTreeSet<TxnId> = active
            .values()
            .filter(|t| t.state == TxnState::Active && t.id != txn_id)
            .map(|t| t.id)
            .collect();
        Ok(ReadView::new(txn_id, txn.start_ts, active_ids))
    }

    /// Mutable access to session txn (record reads/writes).
    pub fn with_txn<F, R>(&self, session_id: u64, f: F) -> Result<R, TxnError>
    where
        F: FnOnce(&mut Transaction) -> R,
    {
        let sessions = self.sessions.lock();
        let txn_id = *sessions.get(&session_id).ok_or(TxnError::NoActiveTxn)?;
        drop(sessions);
        let mut active = self.active.lock();
        let txn = active.get_mut(&txn_id).ok_or(TxnError::NoActiveTxn)?;
        if txn.state != TxnState::Active {
            return Err(TxnError::NoActiveTxn);
        }
        Ok(f(txn))
    }

    /// Read key through MVCC + read-your-writes.
    pub fn get(&self, session_id: u64, key: &[u8]) -> Result<Option<Vec<u8>>, TxnError> {
        self.with_txn(session_id, |txn| {
            txn.record_read(key.to_vec());
        })?;
        let view = self.read_view(session_id)?;
        if let Some(own) = self.with_txn(session_id, |txn| match txn.read_own(key) {
            None => None,
            Some(None) => Some(None),
            Some(Some(v)) => Some(Some(v.to_vec())),
        })? {
            return Ok(own);
        }
        Ok(self.mvcc.lock().get(key, &view))
    }

    /// `COMMIT` — SSI check, apply write set with `commit_ts`, purge intents.
    pub fn commit(&self, session_id: u64) -> Result<CommitResult, TxnError> {
        let txn_id = {
            let sessions = self.sessions.lock();
            *sessions.get(&session_id).ok_or(TxnError::NoActiveTxn)?
        };

        self.deadlock.lock().poll(&self.active.lock(), txn_id)?;

        let (write_set, read_set) = {
            let mut active = self.active.lock();
            let txn = active.get(&txn_id).ok_or(TxnError::NoActiveTxn)?;
            match self.ssi.lock().check_commit(txn) {
                SsiDecision::Allow => {}
                SsiDecision::Abort(msg) => {
                    return Err(TxnError::SerializationFailure(msg));
                }
            }
            let txn = active.get_mut(&txn_id).ok_or(TxnError::NoActiveTxn)?;
            txn.state = TxnState::Committed;
            (txn.write_set.clone(), txn.read_set.clone())
        };

        let commit_ts = self.oracle.next();
        let keys_written = write_set.len();
        {
            let mut mvcc = self.mvcc.lock();
            for (key, op) in &write_set {
                match op {
                    WriteOp::Put(v) => mvcc.put(key.clone(), v.clone(), commit_ts),
                    WriteOp::Delete => mvcc.delete(key.clone(), commit_ts),
                }
            }
        }

        let write_keys: BTreeSet<_> = write_set.keys().cloned().collect();
        self.ssi.lock().register_commit(txn_id, write_keys);
        self.deadlock.lock().clear(txn_id);
        self.ssi.lock().purge(txn_id);

        self.active.lock().remove(&txn_id);
        self.sessions.lock().remove(&session_id);

        let _ = (read_set,);
        Ok(CommitResult {
            commit_ts,
            keys_written,
        })
    }

    /// `ROLLBACK` — discard write set and intents.
    pub fn rollback(&self, session_id: u64) -> Result<(), TxnError> {
        let txn_id = {
            let mut sessions = self.sessions.lock();
            sessions.remove(&session_id).ok_or(TxnError::NoActiveTxn)?
        };

        {
            let mut mvcc = self.mvcc.lock();
            mvcc.abort_intents(txn_id);
        }
        self.deadlock.lock().clear(txn_id);
        self.ssi.lock().purge(txn_id);
        self.active.lock().remove(&txn_id);
        Ok(())
    }

    /// Time-travel read (Week 7 API).
    #[must_use]
    pub fn get_at_ts(&self, key: &[u8], read_ts: CommitTs) -> Option<Vec<u8>> {
        self.mvcc.lock().get_at_ts(key, read_ts)
    }

    /// Background GC — prune versions below `min_retain_ts`.
    pub fn gc(&self, min_retain_ts: CommitTs) -> noedb_storage::mvcc::GcStats {
        gc_versions(&mut self.mvcc.lock(), min_retain_ts)
    }

    /// Number of active transactions.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.lock().len()
    }
}
