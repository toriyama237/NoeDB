//! Subquery decorrelation (Phase 5 Week 39).

use noedb_ast::{BinaryOp, ColumnRef, Expr, FromItem, SelectStmt};

use crate::logical::LogicalPlan;
use crate::PlanError;

/// One `EXISTS (SELECT …)` predicate extracted from `WHERE`.
#[derive(Debug, Clone)]
pub struct ExistsSubqueryPred {
    /// Subquery AST.
    pub query: SelectStmt,
    /// `NOT EXISTS` flag.
    pub negated: bool,
}

/// Remove `EXISTS (SELECT …)` predicates from `WHERE`.
#[must_use]
pub fn peel_exists_subqueries(expr: Option<Expr>) -> (Option<Expr>, Vec<ExistsSubqueryPred>) {
    let Some(expr) = expr else {
        return (None, Vec::new());
    };
    let mut subs = Vec::new();
    let rest = peel_exists_expr(expr, &mut subs);
    (rest, subs)
}

fn peel_exists_expr(expr: Expr, subs: &mut Vec<ExistsSubqueryPred>) -> Option<Expr> {
    match expr {
        Expr::Exists { query, negated, .. } => {
            subs.push(ExistsSubqueryPred {
                query: *query,
                negated,
            });
            None
        }
        Expr::Binary {
            op: BinaryOp::And,
            left,
            right,
            span,
        } => {
            let l = peel_exists_expr(*left, subs);
            let r = peel_exists_expr(*right, subs);
            match (l, r) {
                (None, None) => None,
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (Some(a), Some(b)) => Some(Expr::Binary {
                    op: BinaryOp::And,
                    left: Box::new(a),
                    right: Box::new(b),
                    span,
                }),
            }
        }
        other => Some(other),
    }
}

/// Apply decorrelated semi-joins for each extracted `EXISTS (SELECT …)`.
pub fn apply_exists_subqueries(
    plan: LogicalPlan,
    preds: &[ExistsSubqueryPred],
    outer: &SelectStmt,
    cte_scope: &std::collections::HashSet<String>,
) -> Result<LogicalPlan, PlanError> {
    let mut plan = plan;
    let outer_tables = table_names(outer);
    for pred in preds {
        plan = decorrelate_exists(plan, pred, &outer_tables, cte_scope)?;
    }
    Ok(plan)
}

fn decorrelate_exists(
    plan: LogicalPlan,
    pred: &ExistsSubqueryPred,
    outer_tables: &[String],
    cte_scope: &std::collections::HashSet<String>,
) -> Result<LogicalPlan, PlanError> {
    let inner_tables = table_names(&pred.query);
    let (corr, inner_where) =
        split_correlated_where(pred.query.where_clause.clone(), outer_tables, &inner_tables);

    let mut inner_stmt = pred.query.clone();
    inner_stmt.where_clause = inner_where;

    if corr.is_empty() {
        let inner_plan = crate::build::build_select_scoped(&inner_stmt, cte_scope, None)?;
        let has_rows = !matches!(inner_plan, LogicalPlan::Scan { table, .. } if table.is_empty())
            && !inner_tables.is_empty();
        if (pred.negated == has_rows) || (!pred.negated && !has_rows) {
            return Ok(LogicalPlan::Filter {
                input: Box::new(plan),
                predicate: Expr::Literal(noedb_ast::Literal::Boolean(false, pred.query.span)),
            });
        }
        return Ok(plan);
    }

    // Correlated EXISTS: derive the semi-join keys. Prefer the column the
    // subquery projects (legacy path); when the projection is not a plain
    // column — e.g. `EXISTS (SELECT 1 …)` or `SELECT *` — derive the inner key
    // from the correlation predicate and project it so the semi-join can read
    // the value back.
    let (left_key, right_key) = match subquery_column_name(&pred.query) {
        Ok(inner_key) => extract_exists_join_keys(&corr, &inner_key, outer_tables, &inner_tables)
            .unwrap_or_else(|| ("__exists_outer__".into(), inner_key.clone())),
        Err(_) => {
            let (lk, rk) = exists_join_keys(&corr, outer_tables, &inner_tables)
                .ok_or(PlanError::UnsupportedStatement)?;
            inner_stmt.items = vec![bare_column_item(&rk, pred.query.span)];
            (lk, rk)
        }
    };

    let inner_plan = crate::build::build_select_scoped(&inner_stmt, cte_scope, None)?;
    let corr_on = and_exprs(corr);

    Ok(LogicalPlan::SemiJoin {
        left: Box::new(plan),
        right: Box::new(inner_plan),
        left_key,
        right_key,
        corr_on,
        negated: pred.negated,
    })
}

/// Derive `(outer_key, inner_key)` directly from a correlation equality such as
/// `inner.col = outer.col`, used when the `EXISTS` subquery does not project a
/// plain column to key on.
fn exists_join_keys(
    corr: &[Expr],
    outer_tables: &[String],
    inner_tables: &[String],
) -> Option<(String, String)> {
    for expr in corr {
        let Expr::Binary {
            op: BinaryOp::Eq,
            left,
            right,
            ..
        } = expr
        else {
            continue;
        };
        match (
            side_column(left, outer_tables, inner_tables),
            side_column(right, outer_tables, inner_tables),
        ) {
            (Some((true, outer_col)), Some((false, inner_col)))
            | (Some((false, inner_col)), Some((true, outer_col))) => {
                return Some((
                    unqualified_column(&outer_col),
                    unqualified_column(&inner_col),
                ));
            }
            _ => {}
        }
    }
    None
}

/// Build a bare `column` projection item for a synthesised inner projection.
fn bare_column_item(col: &str, span: noedb_lexer::Span) -> noedb_ast::SelectItem {
    noedb_ast::SelectItem {
        expr: Expr::Column(ColumnRef::Named {
            table: None,
            column: noedb_ast::Ident::new(col.to_string(), span),
        }),
        alias: None,
    }
}

fn extract_exists_join_keys(
    corr: &[Expr],
    inner_key: &str,
    outer_tables: &[String],
    inner_tables: &[String],
) -> Option<(String, String)> {
    for expr in corr {
        let Expr::Binary {
            op: BinaryOp::Eq,
            left,
            right,
            ..
        } = expr
        else {
            continue;
        };
        if let Some((outer, inner)) = corr_pair(left, right, outer_tables, inner_tables, inner_key)
        {
            return Some((outer, inner));
        }
    }
    None
}

fn corr_pair(
    left: &Expr,
    right: &Expr,
    outer_tables: &[String],
    inner_tables: &[String],
    inner_key: &str,
) -> Option<(String, String)> {
    match (
        side_column(left, outer_tables, inner_tables),
        side_column(right, outer_tables, inner_tables),
    ) {
        (Some((true, outer_col)), Some((false, inner_col)))
            if inner_col == inner_key || inner_col.ends_with(&format!(".{inner_key}")) =>
        {
            Some((unqualified_column(&outer_col), inner_key.to_string()))
        }
        (Some((false, inner_col)), Some((true, outer_col)))
            if inner_col == inner_key || inner_col.ends_with(&format!(".{inner_key}")) =>
        {
            Some((unqualified_column(&outer_col), inner_key.to_string()))
        }
        _ => None,
    }
}

fn side_column(
    expr: &Expr,
    outer_tables: &[String],
    inner_tables: &[String],
) -> Option<(bool, String)> {
    let (table, col) = column_ref_parts(expr)?;
    let table = table?;
    if outer_tables.iter().any(|t| t.eq_ignore_ascii_case(&table)) {
        return Some((true, col));
    }
    if inner_tables.iter().any(|t| t.eq_ignore_ascii_case(&table)) {
        return Some((false, col));
    }
    None
}

fn column_ref_parts(expr: &Expr) -> Option<(Option<String>, String)> {
    match expr {
        Expr::Column(ColumnRef::Named { table, column }) => Some((
            table.as_ref().map(|t| t.value.clone()),
            column.value.clone(),
        )),
        _ => None,
    }
}

fn unqualified_column(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

/// One `IN (SELECT …)` predicate extracted from `WHERE`.
#[derive(Debug, Clone)]
pub struct InSubqueryPred {
    /// Left-hand side (`expr IN …`).
    pub outer_expr: Expr,
    /// Subquery AST.
    pub query: SelectStmt,
    /// `NOT IN` flag.
    pub negated: bool,
}

/// Remove `IN (SELECT …)` predicates from `WHERE`, returning the rest and extracted subs.
#[must_use]
pub fn peel_in_subqueries(expr: Option<Expr>) -> (Option<Expr>, Vec<InSubqueryPred>) {
    let Some(expr) = expr else {
        return (None, Vec::new());
    };
    let mut subs = Vec::new();
    let rest = peel_expr(expr, &mut subs);
    (rest, subs)
}

fn peel_expr(expr: Expr, subs: &mut Vec<InSubqueryPred>) -> Option<Expr> {
    match expr {
        Expr::InSubquery {
            expr,
            query,
            negated,
            ..
        } => {
            subs.push(InSubqueryPred {
                outer_expr: *expr,
                query: *query,
                negated,
            });
            None
        }
        Expr::Binary {
            op: BinaryOp::And,
            left,
            right,
            span,
        } => {
            let l = peel_expr(*left, subs);
            let r = peel_expr(*right, subs);
            match (l, r) {
                (None, None) => None,
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (Some(a), Some(b)) => Some(Expr::Binary {
                    op: BinaryOp::And,
                    left: Box::new(a),
                    right: Box::new(b),
                    span,
                }),
            }
        }
        other => Some(other),
    }
}

/// Apply decorrelated semi-joins for each extracted `IN (SELECT …)`.
pub fn apply_in_subqueries(
    plan: LogicalPlan,
    preds: &[InSubqueryPred],
    outer: &SelectStmt,
    cte_scope: &std::collections::HashSet<String>,
) -> Result<LogicalPlan, PlanError> {
    let mut plan = plan;
    let outer_tables = table_names(outer);
    for pred in preds {
        plan = decorrelate_one(plan, pred, &outer_tables, cte_scope)?;
    }
    Ok(plan)
}

fn decorrelate_one(
    plan: LogicalPlan,
    pred: &InSubqueryPred,
    outer_tables: &[String],
    cte_scope: &std::collections::HashSet<String>,
) -> Result<LogicalPlan, PlanError> {
    let left_key = expr_column_name(&pred.outer_expr).ok_or(PlanError::UnsupportedStatement)?;
    let inner_key = subquery_column_name(&pred.query)?;

    let inner_tables = table_names(&pred.query);
    let (corr, inner_where) =
        split_correlated_where(pred.query.where_clause.clone(), outer_tables, &inner_tables);

    let mut inner_stmt = pred.query.clone();
    inner_stmt.where_clause = inner_where;
    let inner_plan = crate::build::build_select_scoped(&inner_stmt, cte_scope, None)?;

    let corr_on = and_exprs(corr);

    Ok(LogicalPlan::SemiJoin {
        left: Box::new(plan),
        right: Box::new(inner_plan),
        left_key,
        right_key: inner_key,
        corr_on,
        negated: pred.negated,
    })
}

fn table_names(stmt: &SelectStmt) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(from) = &stmt.from {
        push_from_item(from, &mut names);
    }
    for j in &stmt.joins {
        push_table_ref(&j.table, &mut names);
    }
    names
}

fn push_from_item(from: &FromItem, names: &mut Vec<String>) {
    match from {
        FromItem::Table(t) => push_table_ref(t, names),
        FromItem::Subquery { alias, .. } => names.push(alias.value.clone()),
    }
}

fn push_table_ref(t: &noedb_ast::TableRef, names: &mut Vec<String>) {
    names.push(t.name.value.clone());
    if let Some(a) = &t.alias {
        names.push(a.value.clone());
    }
}

fn expr_column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(ColumnRef::Named { column, .. }) => Some(column.value.clone()),
        _ => None,
    }
}

fn subquery_column_name(query: &SelectStmt) -> Result<String, PlanError> {
    let item = query.items.first().ok_or(PlanError::UnsupportedStatement)?;
    if let Expr::Column(ColumnRef::Named { column, .. }) = &item.expr {
        Ok(column.value.clone())
    } else if let Some(alias) = &item.alias {
        Ok(alias.value.clone())
    } else {
        Err(PlanError::UnsupportedStatement)
    }
}

fn split_correlated_where(
    where_clause: Option<Expr>,
    outer_tables: &[String],
    inner_tables: &[String],
) -> (Vec<Expr>, Option<Expr>) {
    let Some(expr) = where_clause else {
        return (Vec::new(), None);
    };
    let mut corr = Vec::new();
    let mut rest = Vec::new();
    for part in flatten_and(expr) {
        if is_correlation_eq(&part, outer_tables, inner_tables) {
            corr.push(part);
        } else {
            rest.push(part);
        }
    }
    (corr, and_exprs_opt(rest))
}

fn flatten_and(expr: Expr) -> Vec<Expr> {
    match expr {
        Expr::Binary {
            op: BinaryOp::And,
            left,
            right,
            ..
        } => {
            let mut v = flatten_and(*left);
            v.extend(flatten_and(*right));
            v
        }
        other => vec![other],
    }
}

fn is_correlation_eq(expr: &Expr, outer_tables: &[String], inner_tables: &[String]) -> bool {
    let Expr::Binary {
        op: BinaryOp::Eq,
        left,
        right,
        ..
    } = expr
    else {
        return false;
    };
    let lo = column_qualifier(left);
    let ro = column_qualifier(right);
    matches!(
        (&lo, &ro),
        (Some(o), Some(i)) if outer_tables.contains(o) && inner_tables.contains(i)
    ) || matches!(
        (&lo, &ro),
        (Some(i), Some(o)) if inner_tables.contains(i) && outer_tables.contains(o)
    )
}

fn column_qualifier(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(ColumnRef::Named { table: Some(t), .. }) => Some(t.value.clone()),
        _ => None,
    }
}

fn and_exprs(mut exprs: Vec<Expr>) -> Option<Expr> {
    if exprs.is_empty() {
        return None;
    }
    let first = exprs.remove(0);
    Some(exprs.into_iter().fold(first, |acc, e| {
        let span = acc.span();
        Expr::Binary {
            op: BinaryOp::And,
            left: Box::new(acc),
            right: Box::new(e),
            span,
        }
    }))
}

fn and_exprs_opt(exprs: Vec<Expr>) -> Option<Expr> {
    and_exprs(exprs)
}
