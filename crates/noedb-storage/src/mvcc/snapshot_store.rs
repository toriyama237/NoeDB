//! Snapshot-isolated read view over LSM + MVCC + txn write set.

use std::collections::BTreeMap;
use std::ops::Bound;

use crate::engine::StorageEngine;
use crate::error::StorageError;
use crate::lsm::LsmTree;
use crate::mvcc::{MvccMemTable, ReadView};

/// Pending txn overlay: delete (`None`) or put (`Some(bytes)`).
type TxnWriteSet = BTreeMap<Vec<u8>, Option<Vec<u8>>>;
/// Optional read hook for SSI tracking.
type ReadHook<'a> = Box<dyn Fn(&[u8]) + 'a>;

/// Merged storage view for snapshot reads (SI + read-your-writes).
pub struct SnapshotStore<'a> {
    base: &'a LsmTree,
    mvcc: &'a MvccMemTable,
    view: ReadView,
    /// Pending txn writes: `None` = delete, `Some(v)` = put.
    writes: Option<TxnWriteSet>,
    /// Record reads for SSI (optional).
    on_read: Option<ReadHook<'a>>,
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
            merged.entry(k.clone()).or_insert_with(|| {
                if let Ok(Some(vv)) = self.base.get_visible(&k, &self.view) {
                    vv
                } else {
                    v
                }
            });
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

    fn range<'b>(
        &'b self,
        start: &'b [u8],
        end: &'b [u8],
    ) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + 'b {
        let in_range = |k: &[u8]| k >= start && (end.is_empty() || k < end);
        // All three sources yield keys in ascending order, so a 3-way
        // merge replaces the extra BTreeMap this method used to build
        // (one insert + two allocations per row on every table scan).
        let overlay: Vec<(Vec<u8>, Option<Vec<u8>>)> =
            self.writes.as_ref().map_or_else(Vec::new, |writes| {
                writes
                    .range::<[u8], _>((Bound::Included(start), Bound::Unbounded))
                    .take_while(|(k, _)| in_range(k))
                    .map(|(k, op)| (k.clone(), op.clone()))
                    .collect()
            });
        MergedRange {
            overlay: overlay.into_iter().peekable(),
            mvcc: self
                .mvcc
                .scan_range(&self.view, start, end)
                .into_iter()
                .peekable(),
            base: self.base.range_visible(start, end, &self.view).peekable(),
        }
    }

    fn range_keys<'b>(
        &'b self,
        start: &'b [u8],
        end: &'b [u8],
    ) -> impl Iterator<Item = Vec<u8>> + 'b {
        let in_range = |k: &[u8]| k >= start && (end.is_empty() || k < end);
        // Same 3-way merge as `range`, but no source materializes values
        // (`Vec::new()` placeholders never allocate).
        let overlay: Vec<(Vec<u8>, Option<Vec<u8>>)> =
            self.writes.as_ref().map_or_else(Vec::new, |writes| {
                writes
                    .range::<[u8], _>((Bound::Included(start), Bound::Unbounded))
                    .take_while(|(k, _)| in_range(k))
                    .map(|(k, op)| (k.clone(), op.as_ref().map(|_| Vec::new())))
                    .collect()
            });
        let mvcc: Vec<(Vec<u8>, Vec<u8>)> = self
            .mvcc
            .scan_range_keys(&self.view, start, end)
            .into_iter()
            .map(|k| (k, Vec::new()))
            .collect();
        MergedRange {
            overlay: overlay.into_iter().peekable(),
            mvcc: mvcc.into_iter().peekable(),
            base: self
                .base
                .range_visible_keys(start, end, &self.view)
                .map(|k| (k, Vec::new()))
                .peekable(),
        }
        .map(|(k, _)| k)
    }
}

/// 3-way sorted merge for [`SnapshotStore::range`].
///
/// Precedence on equal keys: txn overlay (put or delete) beats the MVCC
/// memtable, which beats the base LSM.
struct MergedRange<B: Iterator<Item = (Vec<u8>, Vec<u8>)>> {
    overlay: std::iter::Peekable<std::vec::IntoIter<(Vec<u8>, Option<Vec<u8>>)>>,
    mvcc: std::iter::Peekable<std::vec::IntoIter<(Vec<u8>, Vec<u8>)>>,
    base: std::iter::Peekable<B>,
}

impl<B: Iterator<Item = (Vec<u8>, Vec<u8>)>> MergedRange<B> {
    /// Drop pending entries for `key` from lower-precedence sources.
    fn skip_shadowed(&mut self, key: &[u8], skip_mvcc: bool) {
        if skip_mvcc {
            while self.mvcc.peek().is_some_and(|(k, _)| k.as_slice() == key) {
                self.mvcc.next();
            }
        }
        while self.base.peek().is_some_and(|(k, _)| k.as_slice() == key) {
            self.base.next();
        }
    }
}

impl<B: Iterator<Item = (Vec<u8>, Vec<u8>)>> Iterator for MergedRange<B> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            #[derive(PartialEq, Eq, PartialOrd, Ord)]
            enum Src {
                Overlay,
                Mvcc,
                Base,
            }
            let candidates = [
                self.overlay.peek().map(|(k, _)| (k, Src::Overlay)),
                self.mvcc.peek().map(|(k, _)| (k, Src::Mvcc)),
                self.base.peek().map(|(k, _)| (k, Src::Base)),
            ];
            // Smallest key wins; on ties `Src` order encodes precedence.
            let (_, src) = candidates.into_iter().flatten().min()?;
            match src {
                Src::Overlay => {
                    let (k, op) = self.overlay.next()?;
                    self.skip_shadowed(&k, true);
                    if let Some(v) = op {
                        return Some((k, v));
                    }
                    // Deleted in-txn: swallow and continue.
                }
                Src::Mvcc => {
                    let (k, v) = self.mvcc.next()?;
                    self.skip_shadowed(&k, false);
                    return Some((k, v));
                }
                Src::Base => return self.base.next(),
            }
        }
    }
}
