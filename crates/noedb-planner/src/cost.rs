//! Cost model v1 (Week 24).

#![allow(clippy::cast_precision_loss, clippy::suboptimal_flops)]

use crate::physical::PhysicalPlan;

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

/// Table statistics for costing.
#[derive(Debug, Clone)]
pub struct PlanStats {
    /// Default row count when unknown.
    pub default_rows: u64,
    /// Learned row counts from prior executions.
    pub table_rows: std::collections::HashMap<String, u64>,
}

impl Default for PlanStats {
    fn default() -> Self {
        Self {
            default_rows: 10_000,
            table_rows: std::collections::HashMap::new(),
        }
    }
}

impl PlanStats {
    /// Row estimate for `table`.
    #[must_use]
    pub fn rows_for(&self, table: &str) -> u64 {
        self.table_rows
            .get(table)
            .copied()
            .unwrap_or(self.default_rows)
    }
}

/// Estimate total cost of a physical plan.
#[must_use]
pub fn estimate(plan: &PhysicalPlan, stats: &PlanStats) -> f64 {
    match plan {
        PhysicalPlan::SeqScan { table, .. } => stats.rows_for(table) as f64 * SEQ_SCAN_ROW_COST,
        PhysicalPlan::IndexScan { .. } => INDEX_LOOKUP_COST,
        PhysicalPlan::Filter { input, .. } => {
            estimate(input, stats) + stats.default_rows as f64 * FILTER_ROW_COST
        }
        PhysicalPlan::Project { input, .. } => estimate(input, stats),
        PhysicalPlan::HashJoin { left, right, .. } => {
            estimate(left, stats)
                + estimate(right, stats)
                + (stats.default_rows as f64 * HASH_JOIN_ROW_COST * 2.0)
        }
        PhysicalPlan::NestedLoopJoin { left, right, .. } => {
            estimate(left, stats)
                + estimate(right, stats) * stats.default_rows as f64 * NESTED_LOOP_ROW_COST
        }
        PhysicalPlan::MergeJoin { left, right, .. } => {
            estimate(left, stats) + estimate(right, stats) + stats.default_rows as f64
        }
        PhysicalPlan::Aggregate { input, .. } => {
            estimate(input, stats) + stats.default_rows as f64 * 0.5
        }
        PhysicalPlan::Sort { input, .. } => {
            estimate(input, stats) + stats.default_rows as f64 * 1.5
        }
        PhysicalPlan::Limit { input, limit, .. } => {
            estimate(input, stats).min(*limit as f64 * SEQ_SCAN_ROW_COST)
        }
        PhysicalPlan::Window { input, windows } => {
            estimate(input, stats) + windows.len() as f64 * stats.default_rows as f64 * 0.8
        }
        PhysicalPlan::SemiJoin { left, right, .. } => {
            estimate(left, stats)
                + estimate(right, stats)
                + stats.default_rows as f64 * NESTED_LOOP_ROW_COST
        }
        PhysicalPlan::CteScan { .. } => 0.5,
    }
}
