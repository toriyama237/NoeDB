//! AST → logical plan (Week 18).

use noedb_ast::{SelectStmt, Statement};

use crate::logical::LogicalPlan;
use crate::subquery::{apply_in_subqueries, peel_in_subqueries};
use crate::window::wrap_window;
use crate::PlanError;

/// Build a logical plan from a parsed `SELECT`.
#[allow(clippy::option_if_let_else, clippy::unnecessary_wraps)]
pub fn build_select(stmt: &SelectStmt) -> Result<LogicalPlan, PlanError> {
    let mut plan = match &stmt.from {
        Some(table) => LogicalPlan::Scan {
            table: table.name.value.clone(),
        },
        None => {
            // `SELECT 1` — synthetic single-row scan.
            LogicalPlan::Scan {
                table: String::new(),
            }
        }
    };

    for join in &stmt.joins {
        plan = LogicalPlan::Join {
            left: Box::new(plan),
            right: Box::new(LogicalPlan::Scan {
                table: join.table.name.value.clone(),
            }),
            on: join.on.clone(),
        };
    }

    let (where_rest, in_subs) = peel_in_subqueries(stmt.where_clause.clone());
    if let Some(pred) = where_rest {
        plan = LogicalPlan::Filter {
            input: Box::new(plan),
            predicate: pred,
        };
    }

    plan = apply_in_subqueries(plan, &in_subs, stmt)?;

    let (mut plan, items) = wrap_window(plan, &stmt.items);
    plan = LogicalPlan::Project {
        input: Box::new(plan),
        items,
    };

    Ok(plan)
}

/// Build a logical plan from any supported statement.
pub fn build(stmt: &Statement) -> Result<LogicalPlan, PlanError> {
    match stmt {
        Statement::Select(s) => build_select(s),
        _ => Err(PlanError::UnsupportedStatement),
    }
}
