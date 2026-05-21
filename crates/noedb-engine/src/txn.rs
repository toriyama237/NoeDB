//! Transaction integration for [`LocalEngine`](crate::engine::LocalEngine).

use std::collections::BTreeMap;

use noedb_ast::Statement;
use noedb_planner::execute_sql_on;
use noedb_storage::{SnapshotStore, Version};
use noedb_txn::{TxnError, TxnManager, WriteOp};

use crate::engine::QueryResult;
use crate::error::EngineError;
use crate::machine::row_key;

/// Per-engine transaction state.
#[derive(Default)]
pub struct TxnState {
    manager: TxnManager,
    session_id: u64,
}

impl TxnState {
    /// New manager for session `session_id`.
    #[must_use]
    pub fn new(session_id: u64) -> Self {
        Self {
            manager: TxnManager::new(),
            session_id,
        }
    }

    /// Underlying manager (tests).
    #[must_use]
    pub fn manager(&self) -> &TxnManager {
        &self.manager
    }

    /// Whether a transaction is open.
    #[must_use]
    pub fn in_txn(&self) -> bool {
        self.manager
            .with_txn(self.session_id, |_| ())
            .is_ok()
    }

    /// `BEGIN`.
    pub fn begin(&self) -> Result<(), EngineError> {
        self.manager.begin(self.session_id).map_err(txn_err)?;
        Ok(())
    }

    /// `COMMIT` — MVCC versions durable in LSM (internal keys + WAL).
    pub fn commit(
        &self,
        tree: &mut noedb_storage::LsmTree,
    ) -> Result<(), EngineError> {
        let pending = self.pending_writes()?;
        let result = self.manager.commit(self.session_id).map_err(txn_err)?;
        for (key, op) in pending {
            let version = match op {
                WriteOp::Put(v) => Version::put(result.commit_ts, v),
                WriteOp::Delete => Version::tombstone(result.commit_ts),
            };
            tree.put_version(&key, &version)?;
        }
        let min_retain = result.commit_ts.saturating_sub(1);
        let _ = self.manager.gc(min_retain);
        let _ = tree.gc_mvcc_active(min_retain);
        Ok(())
    }

    /// `ROLLBACK`.
    pub fn rollback(&self) -> Result<(), EngineError> {
        self.manager.rollback(self.session_id).map_err(txn_err)
    }

    fn pending_writes(&self) -> Result<Vec<(Vec<u8>, WriteOp)>, EngineError> {
        let mut out = Vec::new();
        self.manager
            .with_txn(self.session_id, |txn| {
                out = txn.write_set.clone().into_iter().collect();
            })
            .map_err(txn_err)?;
        Ok(out)
    }

    /// Buffer a cell write in the active transaction.
    pub fn put_row(
        &self,
        table: &str,
        row: &str,
        column: &str,
        value: &[u8],
    ) -> Result<(), EngineError> {
        let key = row_key(table, row, column);
        self.manager
            .with_txn(self.session_id, |txn| {
                txn.put(key, value.to_vec());
            })
            .map_err(txn_err)?;
        Ok(())
    }

    /// Execute `SELECT` under snapshot isolation when a txn is open.
    pub fn execute_select(
        &self,
        stmt: &Statement,
        tree: &noedb_storage::LsmTree,
    ) -> Result<QueryResult, EngineError> {
        let view = self.manager.read_view(self.session_id).map_err(txn_err)?;
        let writes = self.pending_write_overlay()?;
        let mvcc = self.manager.mvcc();
        let session = self.session_id;
        let mgr = &self.manager;
        let snap = SnapshotStore::new(tree, &mvcc, view, writes).with_read_hook(move |key| {
            let _ = mgr.with_txn(session, |txn| {
                txn.record_read(key.to_vec());
            });
        });
        let records = execute_sql_on(stmt, &snap, tree)?;
        Ok(QueryResult::from_records(&records))
    }

    fn pending_write_overlay(
        &self,
    ) -> Result<Option<BTreeMap<Vec<u8>, Option<Vec<u8>>>>, EngineError> {
        let mut map: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
        self.manager
            .with_txn(self.session_id, |txn| {
                for (k, op) in &txn.write_set {
                    map.insert(
                        k.clone(),
                        match op {
                            WriteOp::Put(v) => Some(v.clone()),
                            WriteOp::Delete => None,
                        },
                    );
                }
            })
            .map_err(txn_err)?;
        if map.is_empty() {
            Ok(None)
        } else {
            Ok(Some(map))
        }
    }
}

fn txn_err(e: TxnError) -> EngineError {
    match e {
        TxnError::Storage(msg) => EngineError::Storage(noedb_storage::StorageError::Io {
            message: msg,
        }),
        other => EngineError::InvalidSql(match other {
            TxnError::NoActiveTxn => "no active transaction",
            TxnError::TxnAlreadyActive => "transaction already in progress",
            TxnError::SerializationFailure(m) => m,
            TxnError::DeadlockVictim => "deadlock detected",
            TxnError::WaitTimeout => "transaction wait timeout",
            TxnError::Storage(_) => "storage error",
        }),
    }
}
