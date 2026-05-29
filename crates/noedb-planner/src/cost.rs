//! Cost model v2 with column statistics (Week 43).

#![allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]

use noedb_ast::{BinaryOp, ColumnRef, Expr};

use crate::physical::PhysicalPlan;
use crate::stats::TableStats;

/// Per-row sequential scan cost (arbitrary units).
pub const SEQ_SCAN_ROW_COST: f64 = 1.0;
/// Point index lookup base cost.
pub const INDEX_LOOKUP_COST: f64 = 4.0;
/// Per-row filter evaluation.
pub const FILTER_ROW_COST: f64 = 0.05;
/// Hash-join build/probe per row.
pub const HASH_JOIN_ROW_COST: f64 = 1.2;
/// Nested-loop join per left row (right scan).
pub const NESTED_LOOP_ROW_COST: f64 = 2.0;

/// Table/column statistics for costing.
#[derive(Debug, Clone, Default)]
pub struct PlanStats {
    /// Default row count when unknown.
    pub default_rows: u64,
    /// Per-table statistics (`ANALYZE` or runtime feedback).
    pub tables: std::collections::HashMap<String, TableStats>,
}

impl PlanStats {
    /// Row estimate for `table`.
    #[must_use]
    pub fn rows_for(&self, table: &str) -> u64 {
        self.tables
            .get(table)
            .map_or(self.default_rows, |t| t.row_count.max(1))
    }

    /// Table stats if present.
    #[must_use]
    pub fn table(&self, table: &str) -> Option<&TableStats> {
        self.tables.get(table)
    }

    /// Estimated rows for `table.col = const` (uniformity assumption).
    #[must_use]
    pub fn estimated_eq_rows(&self, table: &str, column: &str) -> u64 {
        self.tables
            .get(table)
            .map_or(1, |t| t.estimated_eq_rows(column))
    }

    /// Merge another snapshot (e.g. after `ANALYZE`).
    pub fn merge(&mut self, other: Self) {
        for (name, stats) in other.tables {
            self.tables.insert(name, stats);
        }
    }
}

/// Estimate total cost of a physical plan.
#[must_use]
pub fn estimate(plan: &PhysicalPlan, stats: &PlanStats) -> f64 {
    match plan {
        PhysicalPlan::SeqScan { table, .. } => stats.rows_for(table) as f64 * SEQ_SCAN_ROW_COST,
        PhysicalPlan::IndexScan { table, column, .. } => {
            INDEX_LOOKUP_COST + stats.estimated_eq_rows(table, column) as f64 * SEQ_SCAN_ROW_COST
        }
        PhysicalPlan::Filter { input, predicate } => {
            let out_rows = estimated_filter_rows(input, predicate, stats);
            estimate(input, stats) + out_rows as f64 * FILTER_ROW_COST
        }
        PhysicalPlan::Project { input, .. } => estimate(input, stats),
        PhysicalPlan::HashJoin { left, right, .. } => {
            let l = estimate(left, stats);
            let r = estimate(right, stats);
            l + r
                + (output_rows(left, stats).max(output_rows(right, stats)) as f64
                    * HASH_JOIN_ROW_COST)
        }
        PhysicalPlan::NestedLoopJoin { left, right, .. } => {
            let l_rows = output_rows(left, stats);
            estimate(left, stats) + estimate(right, stats) * l_rows as f64 * NESTED_LOOP_ROW_COST
        }
        PhysicalPlan::MergeJoin { left, right, .. } => {
            estimate(left, stats) + estimate(right, stats) + output_rows(left, stats) as f64
        }
        PhysicalPlan::Aggregate { input, .. } => {
            estimate(input, stats) + output_rows(input, stats) as f64 * 0.5
        }
        PhysicalPlan::Sort { input, .. } => {
            estimate(input, stats) + output_rows(input, stats) as f64 * 1.5
        }
        PhysicalPlan::Limit { input, limit, .. } => {
            estimate(input, stats).min(*limit as f64 * SEQ_SCAN_ROW_COST)
        }
        PhysicalPlan::Window { input, windows } => {
            estimate(input, stats) + windows.len() as f64 * output_rows(input, stats) as f64 * 0.8
        }
        PhysicalPlan::SemiJoin { left, right, .. } => {
            estimate(left, stats)
                + estimate(right, stats)
                + output_rows(left, stats) as f64 * NESTED_LOOP_ROW_COST
        }
        PhysicalPlan::CteScan { .. } => 0.5,
        PhysicalPlan::SetOp { left, right, .. } => {
            estimate(left, stats) + estimate(right, stats) + output_rows(left, stats) as f64 * 0.2
        }
    }
}

/// Whether index point lookup beats a full scan for `col = const`.
#[must_use]
pub fn index_beats_seq_scan(table: &str, column: &str, stats: &PlanStats) -> bool {
    let seq = stats.rows_for(table) as f64 * SEQ_SCAN_ROW_COST;
    let idx = INDEX_LOOKUP_COST + stats.estimated_eq_rows(table, column) as f64 * SEQ_SCAN_ROW_COST;
    idx < seq
}

fn output_rows(plan: &PhysicalPlan, stats: &PlanStats) -> u64 {
    match plan {
        PhysicalPlan::SeqScan { table, .. } => stats.rows_for(table),
        PhysicalPlan::IndexScan { table, column, .. } => stats.estimated_eq_rows(table, column),
        PhysicalPlan::Filter { input, predicate } => estimated_filter_rows(input, predicate, stats),
        PhysicalPlan::Limit { input, limit, .. } => output_rows(input, stats).min(*limit),
        PhysicalPlan::Project { input, .. } | PhysicalPlan::Sort { input, .. } => {
            output_rows(input, stats)
        }
        PhysicalPlan::HashJoin { left, .. } => output_rows(left, stats),
        _ => stats.default_rows,
    }
}

fn estimated_filter_rows(input: &PhysicalPlan, predicate: &Expr, stats: &PlanStats) -> u64 {
    if let PhysicalPlan::SeqScan { table, .. } = input {
        if let Some(col) = equality_column(predicate) {
            return stats.estimated_eq_rows(table, &col);
        }
        return stats.rows_for(table);
    }
    output_rows(input, stats)
}

fn equality_column(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Binary {
            op: BinaryOp::Eq,
            left,
            right,
            ..
        } => column_name(left).or_else(|| column_name(right)),
        Expr::Paren(inner, _) => equality_column(inner),
        _ => None,
    }
}

fn column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(ColumnRef::Named { column, .. }) => Some(column.value.clone()),
        _ => None,
    }
}
