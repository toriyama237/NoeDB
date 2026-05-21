//! In-memory sorted key-value store (LSM Level-0 hot path).
//!
//! [`MemTable`] is the write-absorbing component of the LSM tree. Keys are
//! kept in lexicographic order via [`BTreeMap`] so that flush-to-SSTable and
//! full scans are both cheap.

use core::ops::Bound;
use std::collections::BTreeMap;

use crate::error::StorageError;
use crate::StorageEngine;

/// Default maximum number of entries before the engine should rotate the
/// MemTable (Week 10 will wire this to flush + WAL).
pub const DEFAULT_MAX_ENTRIES: usize = 10_000;

/// An in-memory sorted map backing the LSM write path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemTable {
    map: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl MemTable {
    /// Create an empty MemTable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of live key-value pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the table contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    const fn validate_key(key: &[u8]) -> Result<(), StorageError> {
        if key.is_empty() {
            return Err(StorageError::invalid_input("key must not be empty"));
        }
        Ok(())
    }

    /// Insert or overwrite `key` → `value`.
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        Self::validate_key(key)?;
        self.map.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    /// Look up `key`. Returns `None` when absent.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        Self::validate_key(key)?;
        Ok(self.map.get(key).cloned())
    }

    /// Remove `key` if present. Returns whether a value was removed.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, StorageError> {
        Self::validate_key(key)?;
        Ok(self.map.remove(key).is_some())
    }

    /// Iterate all entries in ascending key order.
    pub fn iter(&self) -> MemTableIter<'_> {
        MemTableIter {
            inner: self.map.iter(),
        }
    }

    /// Iterate entries whose keys fall in `[start, end)` (lexicographic).
    pub fn range(&self, start: &[u8], end: &[u8]) -> MemTableRangeIter<'_> {
        MemTableRangeIter {
            inner: self
                .map
                .range::<[u8], _>((Bound::Included(start), Bound::Excluded(end))),
        }
    }
}

impl StorageEngine for MemTable {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        Self::get(self, key)
    }

    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        Self::put(self, key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<bool, Self::Error> {
        Self::delete(self, key)
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        self.map.iter().map(|(k, v)| (k.clone(), v.clone()))
    }
}

impl<'a> IntoIterator for &'a MemTable {
    type Item = (Vec<u8>, Vec<u8>);
    type IntoIter = MemTableIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over all entries in a [`MemTable`], ascending by key.
pub struct MemTableIter<'a> {
    inner: std::collections::btree_map::Iter<'a, Vec<u8>, Vec<u8>>,
}

impl Iterator for MemTableIter<'_> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k.clone(), v.clone()))
    }
}

/// Iterator over a half-open key range in a [`MemTable`].
pub struct MemTableRangeIter<'a> {
    inner: std::collections::btree_map::Range<'a, Vec<u8>, Vec<u8>>,
}

impl Iterator for MemTableRangeIter<'_> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k.clone(), v.clone()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn put_get_delete_round_trip() {
        let mut mt = MemTable::new();
        mt.put(b"user:1", b"rykiel").unwrap();
        assert_eq!(mt.get(b"user:1").unwrap(), Some(b"rykiel".to_vec()));
        assert!(mt.delete(b"user:1").unwrap());
        assert_eq!(mt.get(b"user:1").unwrap(), None);
    }

    #[test]
    fn rejects_empty_key() {
        let mut mt = MemTable::new();
        assert_eq!(
            mt.put(b"", b"v"),
            Err(StorageError::invalid_input("key must not be empty"))
        );
    }

    #[test]
    fn iter_is_lexicographically_sorted() {
        let mut mt = MemTable::new();
        for key in [b"z", b"a", b"m"] {
            mt.put(key, key).unwrap();
        }
        let keys: Vec<_> = mt.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, vec![b"a".to_vec(), b"m".to_vec(), b"z".to_vec()]);
    }

    #[test]
    fn range_scan_is_half_open() {
        let mut mt = MemTable::new();
        for i in 0..5u8 {
            mt.put(&[i], &[i]).unwrap();
        }
        let keys: Vec<_> = mt.range(&[1], &[4]).map(|(k, _)| k).collect();
        assert_eq!(keys, vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn thousand_puts_are_all_readable() {
        let mut mt = MemTable::new();
        for i in 0..1_000u32 {
            let key = format!("key:{i:04}");
            let val = format!("val:{i}");
            mt.put(key.as_bytes(), val.as_bytes()).unwrap();
        }
        assert_eq!(mt.len(), 1_000);
        for i in 0..1_000u32 {
            let key = format!("key:{i:04}");
            let val = format!("val:{i}");
            assert_eq!(mt.get(key.as_bytes()).unwrap(), Some(val.into_bytes()));
        }
        let mut last = None;
        for (k, _) in &mt {
            if let Some(prev) = last {
                assert!(prev < k);
            }
            last = Some(k);
        }
    }
}
