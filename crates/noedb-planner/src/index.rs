//! Secondary B-tree indexes over [`LsmTree`] (Week 20).
//!
//! Index entries:
//!
//! - Point key: `\\x02idx\\0{table}\\0{column}\\0{value}` → `row_id` (O(1) lookup).
//! - Duplicate entries: `\\x02idx\\0{table}\\0{column}\\0{value}\\0{row_id}` → empty.
//! - Metadata: `\\x02idx_meta\\0{table}\\0{column}` → `b"1"`.

use std::collections::BTreeMap;

use noedb_storage::{prefix_end, LsmTree, StorageEngine, StorageError};

const META_PREFIX: &[u8] = b"\x02idx_meta\0";
const ENTRY_PREFIX: &[u8] = b"\x02idx\0";

/// Ordered B-tree secondary index (in-memory view + LSM persistence).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BTreeIndex {
    /// Indexed key → row ids (duplicates allowed).
    entries: BTreeMap<Vec<u8>, Vec<Vec<u8>>>,
}

impl BTreeIndex {
    /// Empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `(key → row_id)`.
    pub fn insert(&mut self, key: Vec<u8>, row_id: Vec<u8>) {
        self.entries.entry(key).or_default().push(row_id);
    }

    /// Exact lookup.
    #[must_use]
    pub fn lookup(&self, key: &[u8]) -> Vec<Vec<u8>> {
        self.entries.get(key).cloned().unwrap_or_default()
    }

    /// Full ordered scan.
    pub fn iter(&self) -> impl Iterator<Item = (&Vec<u8>, &Vec<Vec<u8>>)> {
        self.entries.iter()
    }

    /// Number of distinct indexed keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the index is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Secondary index catalog backed by storage.
pub struct SecondaryIndex;

impl SecondaryIndex {
    /// Build (or rebuild) an index by scanning table rows.
    ///
    /// # Errors
    ///
    /// Propagates storage write failures.
    pub fn build(store: &mut LsmTree, table: &str, column: &str) -> Result<(), StorageError> {
        let mut index = BTreeIndex::new();
        let table_prefix = table_key_prefix(table);
        let table_end = prefix_end(&table_prefix);

        for (key, val) in store.range(&table_prefix, &table_end) {
            if !key.starts_with(&table_prefix) {
                continue;
            }
            let rest = &key[table_prefix.len()..];
            let Some(pos) = rest.iter().position(|&b| b == 0) else {
                continue;
            };
            let row_id = &rest[..pos];
            let col = &rest[pos + 1..];
            if col != column.as_bytes() {
                continue;
            }
            index.insert(val.clone(), row_id.to_vec());
        }

        Self::persist(store, table, column, &index)?;
        Ok(())
    }
    /// Whether an index exists for `(table, column)`.
    #[must_use]
    pub fn exists(store: &LsmTree, table: &str, column: &str) -> bool {
        store
            .get(&meta_key(table, column))
            .ok()
            .is_some_and(|v| v.is_some())
    }

    /// Load index into memory (for tests / bulk operations).
    ///
    /// # Errors
    ///
    /// Propagates storage read failures (currently infallible for iteration).
    pub fn load(store: &LsmTree, table: &str, column: &str) -> BTreeIndex {
        let mut index = BTreeIndex::new();
        let prefix = entry_prefix(table, column);
        let end = prefix_end(&prefix);
        for (key, _) in store.range(&prefix, &end) {
            if !key.starts_with(&prefix) {
                continue;
            }
            let rest = &key[prefix.len()..];
            let Some(pos) = rest.iter().position(|&b| b == 0) else {
                continue;
            };
            let indexed_key = rest[..pos].to_vec();
            let row_id = rest[pos + 1..].to_vec();
            index.insert(indexed_key, row_id);
        }
        index
    }

    /// Point lookup: indexed column value → row ids (duplicates included).
    ///
    /// Reads the duplicate-entry keyspace only; entries tombstoned by DML
    /// index maintenance are invisible to the bounded MVCC scan.
    #[must_use]
    pub fn lookup(store: &LsmTree, table: &str, column: &str, key: &[u8]) -> Vec<Vec<u8>> {
        let mut prefix = entry_prefix(table, column);
        prefix.extend_from_slice(key);
        prefix.push(0);
        let end = prefix_end(&prefix);

        let mut row_ids = Vec::new();
        for (k, _) in store.range(&prefix, &end) {
            if !k.starts_with(&prefix) {
                continue;
            }
            let rest = &k[prefix.len()..];
            row_ids.push(rest.to_vec());
        }
        row_ids
    }

    /// Columns of `table` that have a secondary index.
    #[must_use]
    pub fn indexed_columns(store: &LsmTree, table: &str) -> Vec<String> {
        let mut prefix = META_PREFIX.to_vec();
        prefix.extend_from_slice(table.as_bytes());
        prefix.push(0);
        let end = prefix_end(&prefix);
        store
            .range(&prefix, &end)
            .filter(|(k, _)| k.starts_with(&prefix))
            .map(|(k, _)| String::from_utf8_lossy(&k[prefix.len()..]).into_owned())
            .collect()
    }

    /// Storage key of the duplicate-entry record for `(value, row_id)`.
    ///
    /// Engines use this to maintain the index on DML: write the key with an
    /// empty value on insert, tombstone it on delete/update.
    #[must_use]
    pub fn entry_key_for(table: &str, column: &str, value: &[u8], row_id: &[u8]) -> Vec<u8> {
        entry_key(table, column, value, row_id)
    }

    fn persist(
        store: &mut LsmTree,
        table: &str,
        column: &str,
        index: &BTreeIndex,
    ) -> Result<(), StorageError> {
        store.put(&meta_key(table, column), b"1")?;
        for (key, row_ids) in index.iter() {
            for row_id in row_ids {
                let entry_key = entry_key(table, column, key, row_id);
                store.put(&entry_key, b"")?;
            }
        }
        Ok(())
    }
}

fn table_key_prefix(table: &str) -> Vec<u8> {
    let mut p = table.as_bytes().to_vec();
    p.push(0);
    p
}

fn meta_key(table: &str, column: &str) -> Vec<u8> {
    let mut k = META_PREFIX.to_vec();
    k.extend_from_slice(table.as_bytes());
    k.push(0);
    k.extend_from_slice(column.as_bytes());
    k
}

fn entry_prefix(table: &str, column: &str) -> Vec<u8> {
    let mut p = ENTRY_PREFIX.to_vec();
    p.extend_from_slice(table.as_bytes());
    p.push(0);
    p.extend_from_slice(column.as_bytes());
    p.push(0);
    p
}

fn entry_key(table: &str, column: &str, key: &[u8], row_id: &[u8]) -> Vec<u8> {
    let mut k = entry_prefix(table, column);
    k.extend_from_slice(key);
    k.push(0);
    k.extend_from_slice(row_id);
    k
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use noedb_storage::{LsmConfig, LsmTree};

    fn temp_tree() -> (LsmTree, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "noedb-index-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (LsmTree::open(&dir, LsmConfig::default()).unwrap(), dir)
    }

    fn put_row(tree: &mut LsmTree, table: &str, row: &str, col: &str, val: &[u8]) {
        let mut key = table.as_bytes().to_vec();
        key.push(0);
        key.extend_from_slice(row.as_bytes());
        key.push(0);
        key.extend_from_slice(col.as_bytes());
        tree.put(&key, val).unwrap();
    }

    #[test]
    fn build_and_lookup_index() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"10");
        put_row(&mut tree, "users", "2", "id", b"20");
        put_row(&mut tree, "users", "1", "name", b"ada");
        SecondaryIndex::build(&mut tree, "users", "id").unwrap();
        assert!(SecondaryIndex::exists(&tree, "users", "id"));
        let rows = SecondaryIndex::lookup(&tree, "users", "id", b"10");
        assert_eq!(rows, vec![b"1".to_vec()]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
