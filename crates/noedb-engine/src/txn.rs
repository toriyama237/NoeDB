//! Transaction integration for [`LocalEngine`](crate::engine::LocalEngine).

use std::collections::BTreeMap;
use std::sync::Arc;

use noedb_ast::Statement;
use noedb_planner::execute_sql_on;
use noedb_storage::{LsmTree, ReadView, StorageEngine, StorageError, Version};
use noedb_txn::{TxnError, TxnManager, WriteOp};
use parking_lot::RwLock;

use crate::engine::QueryResult;
use crate::error::EngineError;
use crate::machine::row_key;

/// Apply `COMMIT` to durable LSM (MVCC internal keys + WAL).
pub(crate) fn commit_to_storage(
    manager: &TxnManager,
    session_id: u64,
    tree: &mut LsmTree,
) -> Result<(), EngineError> {
    let pending = pending_writes(manager, session_id)?;
    let result = manager.commit(session_id).map_err(txn_err)?;
    for (key, op) in pending {
        let version = match op {
            WriteOp::Put(v) => Version::put(result.commit_ts, v),
            WriteOp::Delete => Version::tombstone(result.commit_ts),
        };
        tree.put_version(&key, &version)?;
    }
    let min_retain = result.commit_ts.saturating_sub(1);
    let _ = manager.gc(min_retain);
    let _ = tree.gc_mvcc_active(min_retain);
    Ok(())
}

/// Buffer a cell write in the active transaction.
pub(crate) fn put_row_in_txn(
    manager: &TxnManager,
    session_id: u64,
    table: &str,
    row: &str,
    column: &str,
    value: &[u8],
) -> Result<(), EngineError> {
    let key = row_key(table, row, column);
    manager
        .with_txn(session_id, |txn| {
            txn.put(key, value.to_vec());
        })
        .map_err(txn_err)
}

/// Execute `SELECT` under snapshot isolation when a txn is open.
pub(crate) fn execute_select_in_txn(
    manager: Arc<TxnManager>,
    session_id: u64,
    stmt: &Statement,
    storage: &Arc<RwLock<LsmTree>>,
) -> Result<QueryResult, EngineError> {
    let view = manager.read_view(session_id).map_err(txn_err)?;
    let overlay = build_read_overlay(&manager, session_id, &view)?;
    let tree = storage.read();
    #[allow(clippy::redundant_clone)]
    let mgr = manager.clone();
    let store = TxnOverlayStore {
        base: &tree,
        view,
        overlay,
        on_read: Some(Box::new(move |key| {
            let _ = mgr.with_txn(session_id, |txn| {
                txn.record_read(key.to_vec());
            });
        })),
    };
    let records = execute_sql_on(stmt, &store, &tree)?;
    Ok(QueryResult::from_records(&records))
}

/// Merged MVCC + write-set view (no manager locks held during planner execution).
struct TxnOverlayStore<'a> {
    base: &'a LsmTree,
    view: ReadView,
    overlay: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    on_read: Option<Box<dyn Fn(&[u8]) + 'a>>,
}

impl StorageEngine for TxnOverlayStore<'_> {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(hook) = &self.on_read {
            hook(key);
        }
        if let Some(op) = self.overlay.get(key) {
            return Ok(op.clone());
        }
        self.base.get_visible(key, &self.view)
    }

    fn put(&mut self, _key: &[u8], _value: &[u8]) -> Result<(), StorageError> {
        Err(StorageError::invalid_input(
            "txn overlay store is read-only",
        ))
    }

    fn delete(&mut self, _key: &[u8]) -> Result<bool, StorageError> {
        Err(StorageError::invalid_input(
            "txn overlay store is read-only",
        ))
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
        for (k, v) in StorageEngine::iter(self.base) {
            if let Ok(Some(vv)) = self.base.get_visible(&k, &self.view) {
                merged.insert(k, vv);
            } else {
                merged.insert(k, v);
            }
        }
        for (k, op) in &self.overlay {
            match op {
                Some(v) => {
                    merged.insert(k.clone(), v.clone());
                }
                None => {
                    merged.remove(k);
                }
            }
        }
        merged.into_iter()
    }
}

fn build_read_overlay(
    manager: &TxnManager,
    session_id: u64,
    view: &ReadView,
) -> Result<BTreeMap<Vec<u8>, Option<Vec<u8>>>, EngineError> {
    let mut overlay: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
    {
        let mvcc = manager.mvcc();
        for (k, v) in mvcc.scan(view) {
            overlay.insert(k, Some(v));
        }
    }
    manager
        .with_txn(session_id, |txn| {
            for (k, op) in &txn.write_set {
                overlay.insert(
                    k.clone(),
                    match op {
                        WriteOp::Put(v) => Some(v.clone()),
                        WriteOp::Delete => None,
                    },
                );
            }
        })
        .map_err(txn_err)?;
    Ok(overlay)
}

fn pending_writes(
    manager: &TxnManager,
    session_id: u64,
) -> Result<Vec<(Vec<u8>, WriteOp)>, EngineError> {
    let mut out = Vec::new();
    manager
        .with_txn(session_id, |txn| {
            out = txn.write_set.clone().into_iter().collect();
        })
        .map_err(txn_err)?;
    Ok(out)
}

pub(crate) fn txn_err(e: TxnError) -> EngineError {
    match e {
        TxnError::Storage(msg) => EngineError::Storage(StorageError::Io { message: msg }),
        TxnError::SerializationFailure(msg) => EngineError::SerializationFailure(msg.into()),
        TxnError::DeadlockVictim => EngineError::SerializationFailure("deadlock detected".into()),
        TxnError::WaitTimeout => {
            EngineError::SerializationFailure("transaction wait timeout".into())
        }
        TxnError::NoActiveTxn => EngineError::InvalidSql("no active transaction"),
        TxnError::TxnAlreadyActive => EngineError::InvalidSql("transaction already in progress"),
    }
}
