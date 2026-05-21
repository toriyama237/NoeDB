//! AST → logical plan (Week 18).

use noedb_ast::{SelectStmt, Statement};

use crate::logical::LogicalPlan;
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

    if let Some(pred) = &stmt.where_clause {
        plan = LogicalPlan::Filter {
            input: Box::new(plan),
            predicate: pred.clone(),
        };
    }

    plan = LogicalPlan::Project {
        input: Box::new(plan),
        items: stmt.items.clone(),
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
