//! `EXPLAIN` text rendering (Week 28).

use std::fmt::Write;

use crate::cost::{estimate, PlanStats};
use crate::physical::PhysicalPlan;

/// Render a physical plan as human-readable `EXPLAIN` output.
#[must_use]
pub fn explain(plan: &PhysicalPlan) -> String {
    let stats = PlanStats::default();
    let cost = estimate(plan, &stats);
    let mut out = ExplainWriter::new(cost);
    out.write_plan(plan, 0);
    out.finish()
}

struct ExplainWriter {
    lines: Vec<String>,
    total_cost: f64,
}

impl ExplainWriter {
    const fn new(total_cost: f64) -> Self {
        Self {
            lines: Vec::new(),
            total_cost,
        }
    }

    fn write_plan(&mut self, plan: &PhysicalPlan, indent: usize) {
        let pad = "  ".repeat(indent);
        match plan {
            PhysicalPlan::SeqScan { table, columns } => {
                let cols = columns
                    .as_ref()
                    .map_or_else(|| "*".into(), |c| c.join(", "));
                self.lines.push(format!(
                    "{pad}SeqScan(table={table}, columns=[{cols}])"
                ));
            }
            PhysicalPlan::IndexScan {
                table,
                column,
                point_key,
                columns,
            } => {
                let cols = columns
                    .as_ref()
                    .map_or_else(|| "*".into(), |c| c.join(", "));
                let key = point_key
                    .as_ref()
                    .map_or_else(|| "?".into(), |k| format!("{k:?}"));
                self.lines
                    .push(format!("{pad}IndexScan(table={table}, index={column}, key={key}, columns=[{cols}])"));
            }
            PhysicalPlan::Filter { input, .. } => {
                self.lines.push(format!("{pad}Filter"));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::Project { input, items } => {
                self.lines
                    .push(format!("{pad}Project(cols={})", items.len()));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::HashJoin {
                left,
                right,
                left_key,
                right_key,
                ..
            } => {
                self.lines.push(format!(
                    "{pad}HashJoin(keys={left_key}={right_key})"
                ));
                self.write_plan(left, indent + 1);
                self.write_plan(right, indent + 1);
            }
            PhysicalPlan::NestedLoopJoin { left, right, .. } => {
                self.lines.push(format!("{pad}NestedLoopJoin"));
                self.write_plan(left, indent + 1);
                self.write_plan(right, indent + 1);
            }
            PhysicalPlan::MergeJoin {
                left,
                right,
                left_key,
                right_key,
                ..
            } => {
                self.lines.push(format!(
                    "{pad}MergeJoin(keys={left_key}={right_key})"
                ));
                self.write_plan(left, indent + 1);
                self.write_plan(right, indent + 1);
            }
            PhysicalPlan::Aggregate { group_by, aggs, input } => {
                self.lines.push(format!(
                    "{pad}Aggregate(groups={}, aggs={})",
                    group_by.len(),
                    aggs.len()
                ));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::Sort { input, keys } => {
                self.lines.push(format!("{pad}Sort(keys={})", keys.len()));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::Limit {
                input,
                limit,
                offset,
            } => {
                self.lines
                    .push(format!("{pad}Limit(limit={limit}, offset={offset})"));
                self.write_plan(input, indent + 1);
            }
        }
    }

    fn finish(self) -> String {
        let mut s = String::new();
        for line in &self.lines {
            s.push_str(line);
            s.push('\n');
        }
        let _ = writeln!(s, "Total cost: {:.2}", self.total_cost);
        s
    }
}
