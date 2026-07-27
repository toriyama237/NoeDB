//! DDL storage maintenance.
//!
//! `DROP TABLE` must remove every trace of the table from storage — rows,
//! secondary-index entries, optimizer statistics and the primary-key
//! marker. Leaving any of them behind lets a re-created table with the
//! same name resurrect stale rows or route point lookups to wrong keys.

use noedb_storage::{prefix_end, LsmTree, StorageEngine, StorageError};

/// Delete all storage records belonging to `table`.
///
/// Removes, in order: packed/legacy row records (`{table}\0…`),
/// secondary-index entries and metadata, `ANALYZE` statistics, and the
/// primary-key marker. Returns the number of keys tombstoned.
///
/// # Errors
///
/// Propagates storage write failures; earlier deletions are not rolled
/// back (they are all tombstones for a dropped table, so partial progress
/// is harmless).
pub fn purge_table_data(store: &mut LsmTree, table: &str) -> Result<u64, StorageError> {
    let mut prefixes: Vec<Vec<u8>> = Vec::with_capacity(4);

    // Row records: `{table}\0…` (both packed rows and legacy cells).
    let mut rows = table.as_bytes().to_vec();
    rows.push(0);
    prefixes.push(rows);

    // Secondary-index entries and metadata: `\x02idx\0{table}\0…`,
    // `\x02idx_meta\0{table}\0…` (see `index.rs`).
    for head in [&b"\x02idx\0"[..], &b"\x02idx_meta\0"[..]] {
        let mut p = head.to_vec();
        p.extend_from_slice(table.as_bytes());
        p.push(0);
        prefixes.push(p);
    }

    // Optimizer statistics: `\x03stats\0{table}\0…` (see `stats.rs`).
    let mut stats = b"\x03stats\0".to_vec();
    stats.extend_from_slice(table.as_bytes());
    stats.push(0);
    prefixes.push(stats);

    let mut removed = 0u64;
    for prefix in prefixes {
        let end = prefix_end(&prefix);
        let keys: Vec<Vec<u8>> = store.range_keys(&prefix, &end).collect();
        for key in keys {
            StorageEngine::delete(store, &key)?;
            removed += 1;
        }
    }
    crate::pk::unmark_row_id_column(store, table)?;
    Ok(removed)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use noedb_storage::LsmConfig;

    fn temp_tree(tag: &str) -> (LsmTree, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "noedb-ddl-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (LsmTree::open(&dir, LsmConfig::default()).unwrap(), dir)
    }

    #[test]
    fn purge_removes_rows_but_spares_other_tables() {
        let (mut tree, dir) = temp_tree("purge");
        tree.put(&noedb_storage::packed_row_key("emp", "1"), b"a")
            .unwrap();
        tree.put(&noedb_storage::packed_row_key("emp", "2"), b"b")
            .unwrap();
        tree.put(&noedb_storage::packed_row_key("keep", "1"), b"c")
            .unwrap();
        crate::pk::mark_row_id_column(&mut tree, "emp", "id").unwrap();

        let removed = purge_table_data(&mut tree, "emp").unwrap();
        assert_eq!(removed, 2);
        assert_eq!(
            StorageEngine::get(&tree, &noedb_storage::packed_row_key("emp", "1")).unwrap(),
            None
        );
        assert_eq!(
            StorageEngine::get(&tree, &noedb_storage::packed_row_key("keep", "1")).unwrap(),
            Some(b"c".to_vec())
        );
        assert_eq!(crate::pk::row_id_column(&tree, "emp"), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
