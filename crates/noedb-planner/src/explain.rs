//! `EXPLAIN` text rendering (Week 28).

use std::fmt::Write;

use crate::cost::{estimate, PlanStats};
use crate::physical::PhysicalPlan;

/// Render a physical plan as human-readable `EXPLAIN` output.
#[must_use]
pub fn explain(plan: &PhysicalPlan, stats: &PlanStats) -> String {
    let cost = estimate(plan, stats);
    let mut out = ExplainWriter::new(cost, stats);
    out.write_plan(plan, 0);
    out.finish()
}

struct ExplainWriter<'a> {
    lines: Vec<String>,
    total_cost: f64,
    stats: &'a PlanStats,
}

impl<'a> ExplainWriter<'a> {
    #[allow(clippy::missing_const_for_fn)]
    fn new(total_cost: f64, stats: &'a PlanStats) -> Self {
        Self {
            lines: Vec::new(),
            total_cost,
            stats,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn write_plan(&mut self, plan: &PhysicalPlan, indent: usize) {
        let pad = "  ".repeat(indent);
        match plan {
            PhysicalPlan::SeqScan { table, columns, .. } => {
                let cols = display_columns(columns.as_ref(), Some(table));
                let rows = self.stats.rows_for(table);
                self.lines.push(format!(
                    "{pad}SeqScan(table={table}, rows≈{rows}, columns=[{cols}])"
                ));
            }
            PhysicalPlan::IndexScan {
                table,
                column,
                point_key,
                columns,
            } => {
                let cols = display_columns(columns.as_ref(), Some(table));
                let key = point_key
                    .as_ref()
                    .map_or_else(|| "?".into(), |k| format!("{k:?}"));
                let rows = self.stats.estimated_eq_rows(table, column);
                self.lines.push(format!(
                    "{pad}IndexScan(table={table}, index={column}, key={key}, rows≈{rows}, columns=[{cols}])"
                ));
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
                self.lines
                    .push(format!("{pad}HashJoin(keys={left_key}={right_key})"));
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
                self.lines
                    .push(format!("{pad}MergeJoin(keys={left_key}={right_key})"));
                self.write_plan(left, indent + 1);
                self.write_plan(right, indent + 1);
            }
            PhysicalPlan::Aggregate {
                group_by,
                aggs,
                input,
            } => {
                self.lines.push(format!(
                    "{pad}Aggregate(groups={}, aggs={})",
                    group_by.len(),
                    aggs.len()
                ));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::Sort { input, keys, top_k } => {
                let top = top_k
                    .map(|k| format!(", top_k={k}"))
                    .unwrap_or_default();
                self.lines
                    .push(format!("{pad}Sort(keys={}{top})", keys.len()));
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
            PhysicalPlan::Window { input, windows } => {
                self.lines
                    .push(format!("{pad}Window(funcs={})", windows.len()));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::SemiJoin {
                left,
                right,
                left_key,
                right_key,
                negated,
                ..
            } => {
                let op = if *negated { "NOT IN" } else { "IN" };
                self.lines
                    .push(format!("{pad}SemiJoin({op}, keys={left_key}={right_key})"));
                self.write_plan(left, indent + 1);
                self.write_plan(right, indent + 1);
            }
            PhysicalPlan::CteScan { name, columns, .. } => {
                let cols = display_columns(columns.as_ref(), None);
                self.lines
                    .push(format!("{pad}CteScan(name={name}, columns=[{cols}])"));
            }
            PhysicalPlan::SetOp {
                left,
                right,
                op,
                all,
            } => {
                let op_name = match op {
                    noedb_ast::SetOpKind::Union => "UNION",
                    noedb_ast::SetOpKind::Intersect => "INTERSECT",
                    noedb_ast::SetOpKind::Except => "EXCEPT",
                };
                let all_tag = if *all { " ALL" } else { "" };
                self.lines.push(format!("{pad}SetOp({op_name}{all_tag})"));
                self.write_plan(left, indent + 1);
                self.write_plan(right, indent + 1);
            }
            PhysicalPlan::Dedup { input } => {
                self.lines.push(format!("{pad}Dedup"));
                self.write_plan(input, indent + 1);
            }
            PhysicalPlan::SubqueryScan {
                input,
                prefix,
                columns,
                ..
            } => {
                let cols = display_columns(columns.as_ref(), None);
                self.lines.push(format!(
                    "{pad}SubqueryScan(alias={prefix}, columns=[{cols}])"
                ));
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

/// Format a scan's projected columns for `EXPLAIN`, hiding internal helper
/// columns (e.g. `__sort_0__`) and, when scanning a known table, keeping only
/// that table's own columns so a join plan does not echo every column on both
/// scans.
fn display_columns(columns: Option<&Vec<String>>, table: Option<&str>) -> String {
    let Some(cols) = columns else {
        return "*".into();
    };
    let mut visible: Vec<&str> = cols
        .iter()
        .map(String::as_str)
        .filter(|c| !is_internal_column(c))
        .filter(|c| match (table, c.split_once('.')) {
            // Qualified column on a table scan: keep only matching qualifier.
            (Some(t), Some((qual, _))) => qual == t,
            _ => true,
        })
        .collect();
    visible.dedup();
    if visible.is_empty() {
        return "*".into();
    }
    visible.join(", ")
}

fn is_internal_column(name: &str) -> bool {
    let bare = name.rsplit('.').next().unwrap_or(name);
    bare.starts_with("__") && bare.ends_with("__")
}
