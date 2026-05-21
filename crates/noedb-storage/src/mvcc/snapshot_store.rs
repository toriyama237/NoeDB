//! Snapshot-isolated read view over LSM + MVCC + txn write set.

use std::collections::BTreeMap;

use crate::engine::StorageEngine;
use crate::error::StorageError;
use crate::lsm::LsmTree;
use crate::mvcc::{MvccMemTable, ReadView};

/// Merged storage view for snapshot reads (SI + read-your-writes).
pub struct SnapshotStore<'a> {
    base: &'a LsmTree,
    mvcc: &'a MvccMemTable,
    view: ReadView,
    /// Pending txn writes: `None` = delete, `Some(v)` = put.
    writes: Option<BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    /// Record reads for SSI (optional).
    on_read: Option<Box<dyn Fn(&[u8]) + 'a>>,
}

impl<'a> SnapshotStore<'a> {
    /// Build snapshot reader.
    #[must_use]
    pub fn new(
        base: &'a LsmTree,
        mvcc: &'a MvccMemTable,
        view: ReadView,
        writes: Option<BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    ) -> Self {
        Self {
            base,
            mvcc,
            view,
            writes,
            on_read: None,
        }
    }

    /// Hook invoked on each logical key read (SSI / phantom tracking).
    pub fn with_read_hook(mut self, hook: impl Fn(&[u8]) + 'a) -> Self {
        self.on_read = Some(Box::new(hook));
        self
    }

    fn resolve_key(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(hook) = &self.on_read {
            hook(key);
        }
        if let Some(writes) = &self.writes {
            if let Some(op) = writes.get(key) {
                return Ok(op.clone());
            }
        }
        if let Some(v) = self.mvcc.get(key, &self.view) {
            return Ok(Some(v));
        }
        self.base.get_visible(key, &self.view)
    }
}

impl StorageEngine for SnapshotStore<'_> {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        self.resolve_key(key)
    }

    fn put(&mut self, _key: &[u8], _value: &[u8]) -> Result<(), Self::Error> {
        Err(StorageError::invalid_input("SnapshotStore is read-only"))
    }

    fn delete(&mut self, _key: &[u8]) -> Result<bool, Self::Error> {
        Err(StorageError::invalid_input("SnapshotStore is read-only"))
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        let mut merged: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
        for (k, v) in self.mvcc.scan(&self.view) {
            merged.insert(k, v);
        }
        for (k, v) in StorageEngine::iter(self.base) {
            if !merged.contains_key(&k) {
                if let Ok(Some(vv)) = self.base.get_visible(&k, &self.view) {
                    merged.insert(k, vv);
                } else {
                    merged.insert(k, v);
                }
            }
        }
        if let Some(writes) = &self.writes {
            for (k, op) in writes {
                match op {
                    Some(v) => {
                        merged.insert(k.clone(), v.clone());
                    }
                    None => {
                        merged.remove(k);
                    }
                }
            }
        }
        merged.into_iter()
    }
}
