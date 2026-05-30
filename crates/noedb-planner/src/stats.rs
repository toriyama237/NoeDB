//! Table/column statistics for the cost model (Phase 5 Week 43).

use std::collections::{HashMap, HashSet};

use noedb_storage::{LsmTree, StorageEngine, StorageError};

use crate::cost::PlanStats;

const STATS_PREFIX: &[u8] = b"\x03stats\0";

/// Per-column statistics gathered by `ANALYZE`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColumnStats {
    /// Number of distinct values (NDV).
    pub ndv: u64,
}

/// Per-table statistics.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableStats {
    /// Estimated row count.
    pub row_count: u64,
    /// Column-level stats keyed by column name.
    pub columns: HashMap<String, ColumnStats>,
}

impl TableStats {
    /// NDV for `column`, or `row_count` when unknown (worst-case unique).
    #[must_use]
    #[allow(clippy::or_fun_call)]
    pub fn ndv_for(&self, column: &str) -> u64 {
        self.columns
            .get(column)
            .map_or_else(|| self.row_count.max(1), |c| c.ndv.max(1))
    }

    /// Estimated rows matching `column = constant` (uniform distribution).
    #[must_use]
    pub fn estimated_eq_rows(&self, column: &str) -> u64 {
        let ndv = self.ndv_for(column);
        (self.row_count / ndv).max(1)
    }
}

/// Scan `table` and compute row + column NDV statistics.
#[must_use]
pub fn analyze_table<S: StorageEngine<Error = StorageError>>(store: &S, table: &str) -> TableStats {
    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);

    let mut row_ids: HashSet<Vec<u8>> = HashSet::new();
    let mut col_values: HashMap<String, HashSet<Vec<u8>>> = HashMap::new();

    for (key, val) in StorageEngine::iter(store) {
        if !key.starts_with(&prefix) {
            continue;
        }
        let rest = &key[prefix.len()..];
        let Some(pos) = rest.iter().position(|&b| b == 0) else {
            row_ids.insert(rest.to_vec());
            col_values.entry("value".into()).or_default().insert(val);
            continue;
        };
        let row_id = rest[..pos].to_vec();
        let col = String::from_utf8_lossy(&rest[pos + 1..]).into_owned();
        row_ids.insert(row_id);
        col_values.entry(col).or_default().insert(val);
    }

    let columns = col_values
        .into_iter()
        .map(|(name, set)| {
            (
                name,
                ColumnStats {
                    ndv: u64::try_from(set.len()).unwrap_or(u64::MAX),
                },
            )
        })
        .collect();

    TableStats {
        row_count: u64::try_from(row_ids.len()).unwrap_or(u64::MAX),
        columns,
    }
}

/// Persist statistics for one table into the LSM catalog.
///
/// # Errors
///
/// Storage write failures.
pub fn persist_table_stats(
    store: &mut LsmTree,
    table: &str,
    stats: &TableStats,
) -> Result<(), StorageError> {
    store.put(&table_rows_key(table), &stats.row_count.to_be_bytes())?;
    for (col, col_stats) in &stats.columns {
        store.put(&column_ndv_key(table, col), &col_stats.ndv.to_be_bytes())?;
    }
    Ok(())
}

/// Load all persisted statistics into a [`PlanStats`] snapshot.
#[must_use]
pub fn load_plan_stats(store: &LsmTree) -> PlanStats {
    let mut stats = PlanStats::default();
    let mut tables: HashMap<String, TableStats> = HashMap::new();

    for (key, val) in StorageEngine::iter(store) {
        if !key.starts_with(STATS_PREFIX) {
            continue;
        }
        let rest = &key[STATS_PREFIX.len()..];
        let parts: Vec<&[u8]> = rest.split(|&b| b == 0).collect();
        if parts.is_empty() {
            continue;
        }
        let table = String::from_utf8_lossy(parts[0]).into_owned();
        let entry = tables.entry(table).or_default();
        if parts.len() == 2 && parts[1] == b"rows" && val.len() == 8 {
            entry.row_count = u64::from_be_bytes(val[..8].try_into().unwrap_or([0; 8]));
        } else if parts.len() == 3 && parts[2] == b"ndv" && val.len() == 8 {
            let col = String::from_utf8_lossy(parts[1]).into_owned();
            let ndv = u64::from_be_bytes(val[..8].try_into().unwrap_or([0; 8]));
            entry.columns.insert(col, ColumnStats { ndv });
        }
    }

    stats.tables = tables;
    stats
}

fn table_rows_key(table: &str) -> Vec<u8> {
    let mut k = STATS_PREFIX.to_vec();
    k.extend_from_slice(table.as_bytes());
    k.push(0);
    k.extend_from_slice(b"rows");
    k
}

fn column_ndv_key(table: &str, column: &str) -> Vec<u8> {
    let mut k = STATS_PREFIX.to_vec();
    k.extend_from_slice(table.as_bytes());
    k.push(0);
    k.extend_from_slice(column.as_bytes());
    k.push(0);
    k.extend_from_slice(b"ndv");
    k
}

/// Increment persisted row count for `table` by `delta` (may be negative).
///
/// # Errors
///
/// Storage write failures.
pub fn increment_row_count(
    store: &mut LsmTree,
    table: &str,
    delta: i64,
) -> Result<(), StorageError> {
    let key = table_rows_key(table);
    let current = store
        .get(&key)?
        .map(|b| {
            if b.len() == 8 {
                u64::from_be_bytes(b[..8].try_into().unwrap_or([0; 8]))
            } else {
                0
            }
        })
        .unwrap_or(0);
    let next = if delta.is_negative() {
        current.saturating_sub(delta.unsigned_abs())
    } else {
        current.saturating_add(delta as u64)
    };
    store.put(&key, &next.to_be_bytes())
}
