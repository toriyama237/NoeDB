//! AST → logical plan (Week 18).

use std::collections::HashSet;

use noedb_ast::{ColumnRef, Expr, OrderKey, SelectItem, SelectStmt, Statement, TableRef, WithClause};

use crate::aggregate::maybe_build_aggregate;
use crate::logical::LogicalPlan;
use crate::schema::QuerySchema;
use crate::star::expand_select_items;
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
    t.alias
        .as_ref()
        .map_or_else(|| t.name.value.clone(), |a| a.value.clone())
}

/// Build a logical plan from a parsed `SELECT`.
#[allow(clippy::option_if_let_else, clippy::unnecessary_wraps)]
pub fn build_select(stmt: &SelectStmt) -> Result<LogicalPlan, PlanError> {
    build_select_with_schema(stmt, None)
}

/// Build a `SELECT` plan with optional schema for `*` expansion.
pub fn build_select_with_schema(
    stmt: &SelectStmt,
    schema: Option<&QuerySchema>,
) -> Result<LogicalPlan, PlanError> {
    let cte_scope = cte_names_from(stmt.with_clause.as_ref());
    build_select_scoped(stmt, &cte_scope, schema)
}

/// Build a `SELECT` plan with an explicit CTE name scope (for nested queries).
pub fn build_select_scoped(
    stmt: &SelectStmt,
    cte_scope: &HashSet<String>,
    schema: Option<&QuerySchema>,
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

    let items = expand_select_items(stmt, schema, stmt.items.clone())?;
    let (mut plan, window_items) = wrap_window(plan, &items);

    plan = maybe_build_aggregate(plan, stmt, &window_items)?;

    if !contains_aggregate(&window_items) {
        plan = LogicalPlan::Project {
            input: Box::new(plan),
            items: window_items.clone(),
        };
    }

    let order_items = if contains_aggregate(&window_items) {
        aggregate_output_items(stmt, &window_items)?
    } else {
        window_items
    };

    if !stmt.order_by.is_empty() {
        let keys = resolve_order_keys(&order_items, &stmt.order_by)?;
        plan = LogicalPlan::Sort {
            input: Box::new(plan),
            keys,
        };
    }

    if stmt.limit.is_some() || stmt.offset.is_some() {
        plan = LogicalPlan::Limit {
            input: Box::new(plan),
            limit: stmt.limit.unwrap_or(u64::MAX),
            offset: stmt.offset.unwrap_or(0),
        };
    }

    if let Some(c) = &stmt.compound {
        let right_plan = build_select_scoped(c.right.as_ref(), cte_scope, schema)?;
        plan = LogicalPlan::SetOp {
            left: Box::new(plan),
            right: Box::new(right_plan),
            op: c.op,
            all: c.all,
        };
    }

    Ok(plan)
}

fn contains_aggregate(items: &[SelectItem]) -> bool {
    items.iter().any(|item| {
        matches!(
            &item.expr,
            Expr::Function {
                name,
                over: None,
                ..
            } if name.value.eq_ignore_ascii_case("COUNT")
        )
    })
}

fn aggregate_output_items(
    stmt: &SelectStmt,
    items: &[SelectItem],
) -> Result<Vec<SelectItem>, PlanError> {
    let mut out = Vec::new();
    for expr in &stmt.group_by {
        out.push(SelectItem {
            expr: expr.clone(),
            alias: None,
        });
    }
    for item in items {
        if matches!(
            &item.expr,
            Expr::Function {
                name,
                over: None,
                ..
            } if name.value.eq_ignore_ascii_case("COUNT")
        ) {
            out.push(item.clone());
        }
    }
    Ok(out)
}

fn resolve_order_keys(
    items: &[SelectItem],
    order_by: &[OrderKey],
) -> Result<Vec<(String, bool)>, PlanError> {
    order_by
        .iter()
        .map(|key| {
            let name = resolve_sort_column(items, &key.expr)?;
            Ok((name, key.asc))
        })
        .collect()
}

fn resolve_sort_column(items: &[SelectItem], expr: &Expr) -> Result<String, PlanError> {
    match expr {
        Expr::Column(ColumnRef::Named { table, column }) => {
            for item in items {
                if let Some(alias) = &item.alias {
                    if alias.value.eq_ignore_ascii_case(&column.value) {
                        return Ok(alias.value.clone());
                    }
                }
                if let Expr::Column(ColumnRef::Named {
                    table: item_table,
                    column: item_col,
                }) = &item.expr
                {
                    let table_match = match (table, item_table) {
                        (None, None) => true,
                        (Some(t), Some(it)) => t.value.eq_ignore_ascii_case(&it.value),
                        (None, Some(_)) | (Some(_), None) => false,
                    };
                    if table_match && item_col.value.eq_ignore_ascii_case(&column.value) {
                        return Ok(projection_name(item));
                    }
                }
            }
            if let Some(t) = table {
                Ok(format!("{}.{}", t.value, column.value))
            } else {
                Ok(column.value.clone())
            }
        }
        _ => Err(PlanError::UnsupportedStatement),
    }
}

fn projection_name(item: &SelectItem) -> String {
    if let Some(alias) = &item.alias {
        return alias.value.clone();
    }
    if let Expr::Column(ColumnRef::Named {
        table: Some(t),
        column,
    }) = &item.expr
    {
        return format!("{}.{}", t.value, column.value);
    }
    if let Expr::Column(ColumnRef::Named { column, .. }) = &item.expr {
        return column.value.clone();
    }
    "col".into()
}

fn table_scan(t: &TableRef, cte_scope: &HashSet<String>, qualify: bool) -> LogicalPlan {
    let name = t.name.value.clone();
    let prefix = table_prefix(t, qualify);
    if cte_scope.contains(&name) {
        LogicalPlan::CteScan { name, prefix }
    } else {
        LogicalPlan::Scan {
            table: name,
            prefix,
        }
    }
}

/// Build a logical plan from any supported statement.
pub fn build(stmt: &Statement) -> Result<LogicalPlan, PlanError> {
    build_with_schema(stmt, None)
}

/// Build with optional schema catalog (for `SELECT *`).
pub fn build_with_schema(
    stmt: &Statement,
    schema: Option<&QuerySchema>,
) -> Result<LogicalPlan, PlanError> {
    match stmt {
        Statement::Select(s) => build_select_with_schema(s, schema),
        _ => Err(PlanError::UnsupportedStatement),
    }
}
