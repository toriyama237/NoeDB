//! Aggregate detection and logical-plan wiring (`COUNT`, `GROUP BY`).

use noedb_ast::{ColumnRef, Expr, SelectItem, SelectStmt};

use crate::logical::{AggFunc, LogicalPlan};
use crate::PlanError;

/// Build an [`LogicalPlan::Aggregate`] when the select list contains aggregates.
pub fn maybe_build_aggregate(
    input: LogicalPlan,
    stmt: &SelectStmt,
    items: &[SelectItem],
) -> Result<LogicalPlan, PlanError> {
    let aggs = extract_count_aggs(items)?;
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

fn extract_count_aggs(items: &[SelectItem]) -> Result<Vec<(String, AggFunc)>, PlanError> {
    let mut aggs = Vec::new();
    for item in items {
        let Some(func) = count_func_from_item(&item.expr)? else {
            continue;
        };
        aggs.push((projection_name(item), func));
    }
    Ok(aggs)
}

fn count_func_from_item(expr: &Expr) -> Result<Option<AggFunc>, PlanError> {
    let Expr::Function {
        name,
        args,
        over: None,
        ..
    } = expr
    else {
        return Ok(None);
    };
    if !name.value.eq_ignore_ascii_case("COUNT") {
        return Ok(None);
    }
    Ok(Some(parse_count_args(args)?))
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
        if count_func_from_item(&item.expr)?.is_some() {
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
