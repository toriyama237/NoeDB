//! AST → logical plan (Week 18).

use std::collections::HashSet;

use noedb_ast::{SelectStmt, Statement, TableRef, WithClause};

use crate::logical::LogicalPlan;
use crate::subquery::{apply_in_subqueries, peel_in_subqueries};
use crate::window::wrap_window;
use crate::PlanError;

/// Collect CTE names visible in a `WITH` clause.
#[must_use]
pub fn cte_names_from(with: Option<&WithClause>) -> HashSet<String> {
    let mut names = HashSet::new();
    if let Some(w) = with {
        for cte in &w.ctes {
            names.insert(cte.name.value.clone());
        }
    }
    names
}

fn table_prefix(t: &TableRef, qualify: bool) -> String {
    if !qualify {
        return String::new();
    }
    t.alias.as_ref().map_or_else(
        || t.name.value.clone(),
        |a| a.value.clone(),
    )
}

/// Build a logical plan from a parsed `SELECT`.
#[allow(clippy::option_if_let_else, clippy::unnecessary_wraps)]
pub fn build_select(stmt: &SelectStmt) -> Result<LogicalPlan, PlanError> {
    let cte_scope = cte_names_from(stmt.with_clause.as_ref());
    build_select_scoped(stmt, &cte_scope)
}

/// Build a `SELECT` plan with an explicit CTE name scope (for nested queries).
pub fn build_select_scoped(
    stmt: &SelectStmt,
    cte_scope: &HashSet<String>,
) -> Result<LogicalPlan, PlanError> {
    let qualify = !stmt.joins.is_empty();
    let mut plan = stmt.from.as_ref().map_or_else(
        || LogicalPlan::Scan {
            table: String::new(),
            prefix: String::new(),
        },
        |table| table_scan(table, cte_scope, qualify),
    );

    for join in &stmt.joins {
        plan = LogicalPlan::Join {
            left: Box::new(plan),
            right: Box::new(table_scan(&join.table, cte_scope, true)),
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

    plan = apply_in_subqueries(plan, &in_subs, stmt, cte_scope)?;

    let (mut plan, items) = wrap_window(plan, &stmt.items);
    plan = LogicalPlan::Project {
        input: Box::new(plan),
        items,
    };

    Ok(plan)
}

fn table_scan(t: &TableRef, cte_scope: &HashSet<String>, qualify: bool) -> LogicalPlan {
    let name = t.name.value.clone();
    let prefix = table_prefix(t, qualify);
    if cte_scope.contains(&name) {
        LogicalPlan::CteScan { name, prefix }
    } else {
        LogicalPlan::Scan { table: name, prefix }
    }
}

/// Build a logical plan from any supported statement.
pub fn build(stmt: &Statement) -> Result<LogicalPlan, PlanError> {
    match stmt {
        Statement::Select(s) => build_select(s),
        _ => Err(PlanError::UnsupportedStatement),
    }
}
