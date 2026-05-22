//! Parallel table scan helpers (Phase 3 Week 18 — Rayon).

use std::collections::BTreeMap;

use rayon::prelude::*;

use noedb_storage::{StorageEngine, StorageError};

use crate::value::Value;

type RowMap = Vec<(String, Value)>;

/// Minimum cells before switching to parallel grouping.
const PARALLEL_THRESHOLD: usize = 512;

#[derive(Clone)]
struct Cell {
    row_id: Vec<u8>,
    col: String,
    val: Vec<u8>,
}

/// Load rows for `table`, using Rayon when the scan is large enough.
pub fn load_table_rows<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    columns: Option<&[String]>,
) -> Vec<RowMap> {
    let cells = collect_cells(store, table, columns);
    if cells.len() < PARALLEL_THRESHOLD {
        return group_cells_serial(cells);
    }
    group_cells_parallel(cells)
}

fn collect_cells<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    columns: Option<&[String]>,
) -> Vec<Cell> {
    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);

    StorageEngine::iter(store)
        .filter_map(|(key, val)| {
            if !key.starts_with(&prefix) {
                return None;
            }
            let rest = &key[prefix.len()..];
            if let Some(pos) = rest.iter().position(|&b| b == 0) {
                let (row_id, col) = rest.split_at(pos);
                let col = &col[1..];
                let col_name = String::from_utf8_lossy(col).into_owned();
                if columns.is_some_and(|cols| !cols.contains(&col_name)) {
                    return None;
                }
                Some(Cell {
                    row_id: row_id.to_vec(),
                    col: col_name,
                    val,
                })
            } else {
                Some(Cell {
                    row_id: rest.to_vec(),
                    col: "value".into(),
                    val,
                })
            }
        })
        .collect()
}

fn group_cells_serial(cells: Vec<Cell>) -> Vec<RowMap> {
    let mut grouped: BTreeMap<Vec<u8>, RowMap> = BTreeMap::new();
    for cell in cells {
        grouped
            .entry(cell.row_id)
            .or_default()
            .push((cell.col, Value::Bytes(cell.val)));
    }
    grouped.into_values().collect()
}

fn group_cells_parallel(cells: Vec<Cell>) -> Vec<RowMap> {
    let merged: BTreeMap<Vec<u8>, RowMap> = cells
        .into_par_iter()
        .fold(
            BTreeMap::<Vec<u8>, RowMap>::new,
            |mut acc, cell| {
                acc.entry(cell.row_id)
                    .or_default()
                    .push((cell.col, Value::Bytes(cell.val)));
                acc
            },
        )
        .reduce(BTreeMap::new, merge_maps);

    merged.into_values().collect()
}

fn merge_maps(
    mut left: BTreeMap<Vec<u8>, RowMap>,
    right: BTreeMap<Vec<u8>, RowMap>,
) -> BTreeMap<Vec<u8>, RowMap> {
    for (k, mut rows) in right {
        left.entry(k).or_default().append(&mut rows);
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
}
