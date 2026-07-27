//! Primary-key marker — lets the planner turn `WHERE pk = literal` into a
//! direct storage lookup, without needing a secondary index (v2.4).
//!
//! Row storage keys the packed record as `table\0row_id`, where `row_id`
//! is the string form of the **first column's** value on `INSERT`
//! (see `noedb-engine::dml::execute_insert`). For the common case of a
//! single-column primary key that is also the table's first column,
//! `row_id` and the primary-key value coincide, so `WHERE pk = X` can
//! skip the table scan entirely and fetch `table\0X` directly.
//!
//! The engine persists a marker at `CREATE TABLE` time recording that
//! column name; the planner only needs to read it back — no schema
//! dependency inside `noedb-planner`.

use noedb_storage::LsmTree;

const PK_MARKER_PREFIX: &[u8] = b"\x02pk_meta\0";

/// Storage key for the primary-key marker of `table`.
#[must_use]
fn marker_key(table: &str) -> Vec<u8> {
    let mut key = PK_MARKER_PREFIX.to_vec();
    key.extend_from_slice(table.as_bytes());
    key
}

/// Persist `column` as the row-id-bearing primary-key column of `table`.
///
/// # Errors
///
/// Propagates storage write failures.
pub fn mark_row_id_column(
    store: &mut LsmTree,
    table: &str,
    column: &str,
) -> Result<(), noedb_storage::StorageError> {
    noedb_storage::StorageEngine::put(store, &marker_key(table), column.as_bytes())
}

/// Remove the primary-key marker of `table` (`DROP TABLE`, or a new
/// schema whose first column is no longer the sole primary key).
///
/// A stale marker would route `WHERE old_pk = X` on a re-created table
/// to the wrong storage key, so this must run on every table drop.
///
/// # Errors
///
/// Propagates storage write failures.
pub fn unmark_row_id_column(
    store: &mut LsmTree,
    table: &str,
) -> Result<(), noedb_storage::StorageError> {
    noedb_storage::StorageEngine::delete(store, &marker_key(table)).map(|_| ())
}

/// The column whose value equals the storage `row_id` for `table`, if any.
#[must_use]
pub fn row_id_column(store: &LsmTree, table: &str) -> Option<String> {
    noedb_storage::StorageEngine::get(store, &marker_key(table))
        .ok()
        .flatten()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use noedb_storage::LsmConfig;

    use super::*;

    fn temp_tree() -> (LsmTree, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("noedb-pk-{}", uuid_ish()));
        (
            LsmTree::open(&dir, LsmConfig::default()).expect("open"),
            dir,
        )
    }

    fn uuid_ish() -> u128 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    }

    #[test]
    fn marks_and_reads_back() {
        let (mut tree, dir) = temp_tree();
        assert_eq!(row_id_column(&tree, "employees"), None);
        mark_row_id_column(&mut tree, "employees", "id").expect("mark");
        assert_eq!(row_id_column(&tree, "employees"), Some("id".to_string()));
        assert_eq!(row_id_column(&tree, "other_table"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unmark_clears_marker() {
        let (mut tree, dir) = temp_tree();
        mark_row_id_column(&mut tree, "employees", "id").expect("mark");
        unmark_row_id_column(&mut tree, "employees").expect("unmark");
        assert_eq!(row_id_column(&tree, "employees"), None);
        // Unmarking a table without a marker is a no-op.
        unmark_row_id_column(&mut tree, "never_marked").expect("noop");
        let _ = std::fs::remove_dir_all(dir);
    }
}
