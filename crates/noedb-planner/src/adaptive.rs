//! Adaptive optimizer feedback (Phase 3 Week 23).

use std::collections::HashMap;

/// Runtime statistics collected after query execution.
#[derive(Debug, Clone, Default)]
pub struct ExecutionFeedback {
    /// Observed row counts per table.
    pub table_rows: HashMap<String, u64>,
}

impl ExecutionFeedback {
    /// Record actual rows for `table`.
    pub fn record_table(&mut self, table: &str, rows: u64) {
        self.table_rows
            .entry(table.to_string())
            .and_modify(|c| *c = rows)
            .or_insert(rows);
    }

    /// Learned row estimate (falls back to `default`).
    #[must_use]
    pub fn rows_for(&self, table: &str, default: u64) -> u64 {
        self.table_rows.get(table).copied().unwrap_or(default)
    }
}
