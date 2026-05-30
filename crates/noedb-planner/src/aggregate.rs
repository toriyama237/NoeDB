//! Aggregate detection and logical-plan wiring (`COUNT`, `SUM`, `GROUP BY`).

use noedb_ast::{ColumnRef, Expr, SelectItem, SelectStmt};

use crate::logical::{AggFunc, LogicalPlan};
use crate::PlanError;

/// Build an [`LogicalPlan::Aggregate`] when the select list contains aggregates.
pub fn maybe_build_aggregate(
    input: LogicalPlan,
    stmt: &SelectStmt,
    items: &[SelectItem],
) -> Result<LogicalPlan, PlanError> {
    let aggs = extract_aggs(items)?;
    if aggs.is_empty() {
        return Ok(input);
    }
    validate_grouped_select(items, &stmt.group_by)?;
    let group_by = group_by_columns(&stmt.group_by)?;
    Ok(LogicalPlan::Aggregate {
        input: Box::new(input),
        group_by,
        aggs,
    })
}

fn extract_aggs(items: &[SelectItem]) -> Result<Vec<(String, AggFunc)>, PlanError> {
    let mut aggs = Vec::new();
    for item in items {
        let Some(func) = agg_func_from_item(&item.expr)? else {
            continue;
        };
        aggs.push((projection_name(item), func));
    }
    Ok(aggs)
}

fn agg_func_from_item(expr: &Expr) -> Result<Option<AggFunc>, PlanError> {
    let Expr::Function {
        name,
        args,
        over: None,
        ..
    } = expr
    else {
        return Ok(None);
    };
    Ok(parse_agg_args(&name.value, args))
}

fn parse_agg_args(name: &str, args: &[Expr]) -> Option<AggFunc> {
    match name.to_ascii_uppercase().as_str() {
        "COUNT" => parse_count_args(args).ok(),
        "SUM" | "AVG" | "MIN" | "MAX" => {
            let col = column_arg(args).ok()?;
            Some(match name.to_ascii_uppercase().as_str() {
                "SUM" => AggFunc::Sum(col),
                "AVG" => AggFunc::Avg(col),
                "MIN" => AggFunc::Min(col),
                _ => AggFunc::Max(col),
            })
        }
        _ => None,
    }
}

fn column_arg(args: &[Expr]) -> Result<String, PlanError> {
    match args.len() {
        1 => expr_column_key(&args[0]),
        _ => Err(PlanError::UnsupportedStatement),
    }
}

fn expr_column_key(expr: &Expr) -> Result<String, PlanError> {
    match expr {
        Expr::Column(ColumnRef::Named { column, .. }) => Ok(column.value.clone()),
        Expr::Cast { expr, .. } => expr_column_key(expr),
        _ => Err(PlanError::UnsupportedStatement),
    }
}

fn parse_count_args(args: &[Expr]) -> Result<AggFunc, PlanError> {
    match args.len() {
        0 => Ok(AggFunc::CountStar),
        1 => match &args[0] {
            Expr::Column(ColumnRef::Star { .. })
            | Expr::Column(ColumnRef::QualifiedStar { .. }) => Ok(AggFunc::CountStar),
            Expr::Column(ColumnRef::Named { column, .. }) => {
                Ok(AggFunc::CountCol(column.value.clone()))
            }
            Expr::Literal(_) => Ok(AggFunc::CountStar),
            _ => Err(PlanError::UnsupportedStatement),
        },
        _ => Err(PlanError::UnsupportedStatement),
    }
}

fn validate_grouped_select(items: &[SelectItem], group_by: &[Expr]) -> Result<(), PlanError> {
    if group_by.is_empty() {
        return Ok(());
    }
    let keys: Vec<String> = group_by_columns(group_by)?;
    for item in items {
        if agg_func_from_item(&item.expr)?.is_some() {
            continue;
        }
        let col = column_name_from_expr(&item.expr)?;
        if !keys.iter().any(|k| k == &col || k.ends_with(&format!(".{col}"))) {
            return Err(PlanError::UnsupportedStatement);
        }
    }
    Ok(())
}

fn group_by_columns(group_by: &[Expr]) -> Result<Vec<String>, PlanError> {
    group_by.iter().map(column_name_from_expr).collect()
}

fn column_name_from_expr(expr: &Expr) -> Result<String, PlanError> {
    match expr {
        Expr::Column(ColumnRef::Named { table: Some(t), column }) => {
            Ok(format!("{}.{}", t.value, column.value))
        }
        Expr::Column(ColumnRef::Named { column, .. }) => Ok(column.value.clone()),
        _ => Err(PlanError::UnsupportedStatement),
    }
}

fn projection_name(item: &SelectItem) -> String {
    if let Some(alias) = &item.alias {
        return alias.value.clone();
    }
    if let Expr::Function { name, .. } = &item.expr {
        return name.value.to_ascii_lowercase();
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

/// Map `HAVING` aggregate expressions to post-aggregate output column names.
#[must_use]
pub fn rewrite_having_for_aggregate(having: &Expr, items: &[SelectItem]) -> Expr {
    match having {
        Expr::Function { .. } => match_having_aggregate(having, items),
        Expr::Binary {
            op,
            left,
            right,
            span,
        } => Expr::Binary {
            op: *op,
            left: Box::new(rewrite_having_for_aggregate(left, items)),
            right: Box::new(rewrite_having_for_aggregate(right, items)),
            span: *span,
        },
        Expr::Unary { op, expr, span } => Expr::Unary {
            op: *op,
            expr: Box::new(rewrite_having_for_aggregate(expr, items)),
            span: *span,
        },
        Expr::IsNull {
            expr,
            negated,
            span,
        } => Expr::IsNull {
            expr: Box::new(rewrite_having_for_aggregate(expr, items)),
            negated: *negated,
            span: *span,
        },
        Expr::Between {
            expr,
            low,
            high,
            negated,
            span,
        } => Expr::Between {
            expr: Box::new(rewrite_having_for_aggregate(expr, items)),
            low: Box::new(rewrite_having_for_aggregate(low, items)),
            high: Box::new(rewrite_having_for_aggregate(high, items)),
            negated: *negated,
            span: *span,
        },
        Expr::Paren(inner, span) => {
            Expr::Paren(Box::new(rewrite_having_for_aggregate(inner, items)), *span)
        }
        other => other.clone(),
    }
}

fn match_having_aggregate(expr: &Expr, items: &[SelectItem]) -> Expr {
    for item in items {
        if having_expr_matches(&item.expr, expr) {
            let name = projection_name(item);
            return Expr::Column(ColumnRef::Named {
                table: None,
                column: noedb_ast::Ident::new(name, expr.span()),
            });
        }
    }
    expr.clone()
}

fn having_expr_matches(a: &Expr, b: &Expr) -> bool {
    match (a, b) {
        (
            Expr::Function {
                name: n1,
                args: a1,
                over: o1,
                ..
            },
            Expr::Function {
                name: n2,
                args: a2,
                over: o2,
                ..
            },
        ) => {
            n1 == n2
                && o1.is_none()
                && o2.is_none()
                && a1.len() == a2.len()
                && a1
                    .iter()
                    .zip(a2.iter())
                    .all(|(x, y)| having_expr_matches(x, y))
        }
        (
            Expr::Cast {
                expr: e1,
                data_type: t1,
                ..
            },
            Expr::Cast {
                expr: e2,
                data_type: t2,
                ..
            },
        ) => t1 == t2 && having_expr_matches(e1, e2),
        (
            Expr::Column(ColumnRef::Named {
                table: t1,
                column: c1,
                ..
            }),
            Expr::Column(ColumnRef::Named {
                table: t2,
                column: c2,
                ..
            }),
        ) => t1 == t2 && c1 == c2,
        (
            Expr::Literal(l1),
            Expr::Literal(l2),
        ) => l1 == l2,
        (
            Expr::Binary {
                op: o1,
                left: l1,
                right: r1,
                ..
            },
            Expr::Binary {
                op: o2,
                left: l2,
                right: r2,
                ..
            },
        ) => o1 == o2 && having_expr_matches(l1, l2) && having_expr_matches(r1, r2),
        (
            Expr::Unary {
                op: o1,
                expr: e1,
                ..
            },
            Expr::Unary {
                op: o2,
                expr: e2,
                ..
            },
        ) => o1 == o2 && having_expr_matches(e1, e2),
        (Expr::Paren(e1, _), Expr::Paren(e2, _)) => having_expr_matches(e1, e2),
        _ => false,
    }
}
