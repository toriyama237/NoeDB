//! Transaction integration for [`LocalEngine`](crate::engine::LocalEngine).

use std::collections::BTreeMap;
use std::sync::Arc;

use noedb_ast::Statement;
use noedb_planner::execute_sql_on_with_schema;
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
    manager
        .commit_with_durable(session_id, |commit_ts, pending| {
            for (key, op) in pending {
                let version = match op {
                    WriteOp::Put(v) => Version::put(commit_ts, v.clone()),
                    WriteOp::Delete => Version::tombstone(commit_ts),
                };
                tree.put_version(key, &version)
                    .map_err(|e| TxnError::Storage(e.to_string()))?;
            }
            Ok(())
        })
        .map_err(txn_err)?;
    let min_retain = manager.oracle().now().saturating_sub(1);
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

/// Merged MVCC + write-set view for reads inside a transaction.
pub(crate) struct TxnOverlayStore<'a> {
    base: &'a LsmTree,
    view: ReadView,
    overlay: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
}

impl<'a> TxnOverlayStore<'a> {
    /// Build a snapshot view for `session_id` reads (SELECT and DML scans).
    pub(crate) fn for_session(
        manager: &TxnManager,
        session_id: u64,
        base: &'a LsmTree,
    ) -> Result<TxnOverlayStore<'a>, EngineError> {
        let view = manager.read_view(session_id).map_err(txn_err)?;
        let overlay = build_read_overlay(manager, session_id, &view)?;
        Ok(Self {
            base,
            view,
            overlay,
        })
    }
}

impl StorageEngine for TxnOverlayStore<'_> {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
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

/// Buffer a tombstone for one cell in the active transaction.
pub(crate) fn delete_cell_in_txn(
    manager: &TxnManager,
    session_id: u64,
    key: Vec<u8>,
) -> Result<(), EngineError> {
    manager
        .with_txn(session_id, |txn| {
            txn.delete(key);
        })
        .map_err(txn_err)
}

/// Execute `SELECT` under snapshot isolation when a txn is open.
pub(crate) fn execute_select_in_txn(
    manager: Arc<TxnManager>,
    session_id: u64,
    stmt: &Statement,
    storage: &Arc<RwLock<LsmTree>>,
    schema: &noedb_planner::QuerySchema,
) -> Result<QueryResult, EngineError> {
    let view = manager.read_view(session_id).map_err(txn_err)?;
    let overlay = build_read_overlay(&manager, session_id, &view)?;
    let tree = storage.read();
    #[allow(clippy::redundant_clone)]
    let mgr = manager.clone();
    let store = TxnOverlayStoreWithHook {
        inner: TxnOverlayStore {
            base: &tree,
            view,
            overlay,
        },
        on_read: Some(Box::new(move |key| {
            let _ = mgr.with_txn(session_id, |txn| {
                txn.record_read(key.to_vec());
            });
        })),
    };
    let records = execute_sql_on_with_schema(stmt, &store, &tree, Some(schema))?;
    Ok(QueryResult::from_records(&records))
}

struct TxnOverlayStoreWithHook<'a> {
    inner: TxnOverlayStore<'a>,
    on_read: Option<Box<dyn Fn(&[u8]) + 'a>>,
}

impl StorageEngine for TxnOverlayStoreWithHook<'_> {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(hook) = &self.on_read {
            hook(key);
        }
        self.inner.get(key)
    }

    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.inner.put(key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<bool, StorageError> {
        self.inner.delete(key)
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        self.inner.iter()
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
