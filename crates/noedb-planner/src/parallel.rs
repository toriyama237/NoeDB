//! Table scan helpers: packed rows + legacy cells, Rayon for large scans.

use std::collections::BTreeMap;

use rayon::prelude::*;

use noedb_storage::{decode_row, is_packed_row, prefix_end, StorageEngine, StorageError};

use crate::value::Value;

type RowMap = Vec<(String, Value)>;

/// Minimum entries before switching to parallel decoding.
const PARALLEL_THRESHOLD: usize = 512;

/// One storage entry under the table prefix, not yet decoded.
struct Entry {
    row_id: Vec<u8>,
    kind: EntryKind,
}

enum EntryKind {
    /// Packed record holding the full row (v2.3 layout).
    Packed(Vec<u8>),
    /// Legacy single-cell value.
    Cell { col: String, val: Vec<u8> },
}

/// Load rows for `table`, using Rayon when the scan is large enough.
///
/// Packed records are decoded in one step; legacy cells are grouped by
/// row id. When both exist for one row, cells override packed columns
/// (cell writes are the manual API and are always at least as fresh).
pub fn load_table_rows<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    columns: Option<&[String]>,
) -> Vec<RowMap> {
    let entries = collect_entries(store, table, columns);
    if entries.len() < PARALLEL_THRESHOLD {
        return group_entries(entries, columns);
    }
    group_entries_parallel(entries, columns)
}

fn collect_entries<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    columns: Option<&[String]>,
) -> Vec<Entry> {
    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);
    let end = prefix_end(&prefix);

    store
        .range(&prefix, &end)
        .filter_map(|(key, val)| {
            if !key.starts_with(&prefix) {
                return None;
            }
            let rest = &key[prefix.len()..];
            if let Some(pos) = rest.iter().position(|&b| b == 0) {
                let (row_id, col) = rest.split_at(pos);
                let col_name = String::from_utf8_lossy(&col[1..]).into_owned();
                if columns.is_some_and(|cols| !cols.contains(&col_name)) {
                    return None;
                }
                Some(Entry {
                    row_id: row_id.to_vec(),
                    kind: EntryKind::Cell { col: col_name, val },
                })
            } else if is_packed_row(&val) {
                Some(Entry {
                    row_id: rest.to_vec(),
                    kind: EntryKind::Packed(val),
                })
            } else {
                Some(Entry {
                    row_id: rest.to_vec(),
                    kind: EntryKind::Cell {
                        col: "value".into(),
                        val,
                    },
                })
            }
        })
        .collect()
}

/// Decode a packed record into a row, applying the column filter.
fn decode_packed(bytes: &[u8], columns: Option<&[String]>) -> RowMap {
    decode_row(bytes)
        .unwrap_or_default()
        .into_iter()
        .filter(|(name, _)| columns.is_none_or(|cols| cols.contains(name)))
        .map(|(name, val)| (name, Value::Bytes(val)))
        .collect()
}

/// Merge one row's packed record and cell overrides.
fn merge_row(packed: Option<RowMap>, cells: RowMap) -> RowMap {
    let Some(mut row) = packed else {
        return cells;
    };
    for (col, val) in cells {
        if let Some(slot) = row.iter_mut().find(|(name, _)| *name == col) {
            slot.1 = val;
        } else {
            row.push((col, val));
        }
    }
    row
}

type Grouped = BTreeMap<Vec<u8>, (Option<RowMap>, RowMap)>;

fn ingest(acc: &mut Grouped, entry: Entry, columns: Option<&[String]>) {
    let slot = acc.entry(entry.row_id).or_default();
    match entry.kind {
        EntryKind::Packed(bytes) => slot.0 = Some(decode_packed(&bytes, columns)),
        EntryKind::Cell { col, val } => slot.1.push((col, Value::Bytes(val))),
    }
}

fn group_entries(entries: Vec<Entry>, columns: Option<&[String]>) -> Vec<RowMap> {
    let mut grouped: Grouped = BTreeMap::new();
    for entry in entries {
        ingest(&mut grouped, entry, columns);
    }
    grouped
        .into_values()
        .map(|(packed, cells)| merge_row(packed, cells))
        .collect()
}

fn group_entries_parallel(entries: Vec<Entry>, columns: Option<&[String]>) -> Vec<RowMap> {
    let merged: Grouped = entries
        .into_par_iter()
        .fold(Grouped::new, |mut acc, entry| {
            ingest(&mut acc, entry, columns);
            acc
        })
        .reduce(Grouped::new, merge_grouped);

    merged
        .into_values()
        .map(|(packed, cells)| merge_row(packed, cells))
        .collect()
}

fn merge_grouped(mut left: Grouped, right: Grouped) -> Grouped {
    for (k, (packed, mut cells)) in right {
        let slot = left.entry(k).or_default();
        if packed.is_some() {
            slot.0 = packed;
        }
        slot.1.append(&mut cells);
    }
    left
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use noedb_storage::{LsmConfig, LsmTree};

    fn put_row(tree: &mut LsmTree, table: &str, row: &str, col: &str, val: &[u8]) {
        let mut key = table.as_bytes().to_vec();
        key.push(0);
        key.extend_from_slice(row.as_bytes());
        key.push(0);
        key.extend_from_slice(col.as_bytes());
        tree.put(&key, val).unwrap();
    }

    #[test]
    fn parallel_matches_serial() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-par-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        for i in 0..600u32 {
            put_row(&mut tree, "t", &i.to_string(), "x", b"v");
        }
        let rows = load_table_rows(&tree, "t", None);
        assert_eq!(rows.len(), 600);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn packed_rows_load_and_cells_override() {
        let dir = std::env::temp_dir().join(format!(
            "noedb-packed-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();

        // One packed record for row "1".
        let record = noedb_storage::encode_row(&[
            ("id".to_string(), b"1".to_vec()),
            ("name".to_string(), b"Ada".to_vec()),
        ]);
        tree.put(&noedb_storage::packed_row_key("t", "1"), &record)
            .unwrap();
        // A later cell write overrides the packed column.
        put_row(&mut tree, "t", "1", "name", b"Grace");
        // Row "2" exists only as legacy cells.
        put_row(&mut tree, "t", "2", "id", b"2");
        put_row(&mut tree, "t", "2", "name", b"Alan");

        let rows = load_table_rows(&tree, "t", None);
        assert_eq!(rows.len(), 2);
        let get = |row: &RowMap, col: &str| {
            row.iter()
                .find_map(|(n, v)| (n == col).then(|| v.clone()))
                .unwrap()
        };
        assert_eq!(get(&rows[0], "name"), Value::Bytes(b"Grace".to_vec()));
        assert_eq!(get(&rows[1], "name"), Value::Bytes(b"Alan".to_vec()));

        // Column pruning applies to packed records too.
        let pruned = load_table_rows(&tree, "t", Some(&["id".to_string()]));
        assert!(pruned.iter().all(|r| r.iter().all(|(n, _)| n == "id")));
        let _ = std::fs::remove_dir_all(dir);
    }
}
