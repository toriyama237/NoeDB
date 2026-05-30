//! Expand `SELECT *` / `table.*` using the schema catalog before projection.

use noedb_ast::{ColumnRef, Expr, FromItem, Ident, SelectItem, SelectStmt, TableRef};
use noedb_lexer::Span;

use crate::schema::QuerySchema;
use crate::PlanError;

/// Replace star projections with concrete column references.
pub fn expand_select_items(
    stmt: &SelectStmt,
    schema: Option<&QuerySchema>,
    items: Vec<SelectItem>,
) -> Result<Vec<SelectItem>, PlanError> {
    let qualify = !stmt.joins.is_empty();
    let mut out = Vec::new();
    for item in items {
        match &item.expr {
            Expr::Column(ColumnRef::Star { span }) => {
                out.extend(expand_bare_star(stmt, schema, *span, qualify)?);
            }
            Expr::Column(ColumnRef::QualifiedStar { table, span }) => {
                out.extend(expand_qualified_star(
                    stmt, schema, table, *span, qualify,
                )?);
            }
            _ => out.push(item),
        }
    }
    Ok(out)
}

fn expand_bare_star(
    stmt: &SelectStmt,
    schema: Option<&QuerySchema>,
    span: Span,
    qualify: bool,
) -> Result<Vec<SelectItem>, PlanError> {
    let Some(from) = stmt.from.as_ref() else {
        return Err(PlanError::UnsupportedStatement);
    };
    let mut items = expand_from_item(from, schema, span, qualify)?;
    for join in &stmt.joins {
        items.extend(expand_table_columns(
            &join.table,
            schema,
            span,
            true,
        )?);
    }
    Ok(items)
}

fn expand_qualified_star(
    stmt: &SelectStmt,
    schema: Option<&QuerySchema>,
    qualifier: &Ident,
    span: Span,
    qualify: bool,
) -> Result<Vec<SelectItem>, PlanError> {
    let from = resolve_from_item(stmt, &qualifier.value)?;
    expand_from_item(&from, schema, span, qualify || !stmt.joins.is_empty())
}

fn expand_from_item(
    from: &FromItem,
    schema: Option<&QuerySchema>,
    span: Span,
    qualify: bool,
) -> Result<Vec<SelectItem>, PlanError> {
    match from {
        FromItem::Table(t) => expand_table_columns(t, schema, span, qualify),
        FromItem::Subquery { query, alias, .. } => {
            expand_subquery_columns(query, alias, span, qualify)
        }
    }
}

fn expand_subquery_columns(
    query: &SelectStmt,
    alias: &Ident,
    span: Span,
    qualify: bool,
) -> Result<Vec<SelectItem>, PlanError> {
    let prefix = if qualify {
        alias.value.clone()
    } else {
        String::new()
    };
    Ok(query
        .items
        .iter()
        .map(|item| {
            let col_name = item
                .alias
                .as_ref()
                .map_or_else(|| projection_label(&item.expr), |a| a.value.clone());
            let (table_ref, column, alias_out) = if prefix.is_empty() {
                (
                    None,
                    Ident::new(col_name.clone(), span),
                    None,
                )
            } else {
                (
                    Some(Ident::new(prefix.clone(), span)),
                    Ident::new(col_name.clone(), span),
                    Some(Ident::new(format!("{prefix}.{col_name}"), span)),
                )
            };
            SelectItem {
                expr: Expr::Column(ColumnRef::Named {
                    table: table_ref,
                    column,
                }),
                alias: alias_out,
            }
        })
        .collect())
}

fn projection_label(expr: &Expr) -> String {
    match expr {
        Expr::Column(ColumnRef::Named { column, .. }) => column.value.clone(),
        Expr::Function { name, .. } => name.value.to_ascii_lowercase(),
        _ => "col".into(),
    }
}

fn expand_table_columns(
    table: &TableRef,
    schema: Option<&QuerySchema>,
    span: Span,
    qualify: bool,
) -> Result<Vec<SelectItem>, PlanError> {
    let catalog = schema.ok_or(PlanError::MissingSchema)?;
    let cols = catalog
        .columns_for(&table.name.value)
        .ok_or_else(|| PlanError::UnknownTable {
            name: table.name.value.clone(),
        })?;
    let prefix = if qualify {
        table
            .alias
            .as_ref()
            .map_or_else(|| table.name.value.clone(), |a| a.value.clone())
    } else {
        String::new()
    };
    Ok(cols
        .iter()
        .map(|col| {
            let (table_ref, column, alias) = if prefix.is_empty() {
                (
                    None,
                    Ident::new(col.clone(), span),
                    None,
                )
            } else {
                (
                    Some(Ident::new(prefix.clone(), span)),
                    Ident::new(col.clone(), span),
                    Some(Ident::new(format!("{prefix}.{col}"), span)),
                )
            };
            SelectItem {
                expr: Expr::Column(ColumnRef::Named {
                    table: table_ref,
                    column,
                }),
                alias,
            }
        })
        .collect())
}

fn resolve_from_item(stmt: &SelectStmt, name: &str) -> Result<FromItem, PlanError> {
    if let Some(from) = &stmt.from {
        if from_matches(from, name) {
            return Ok(from.clone());
        }
    }
    for join in &stmt.joins {
        if table_matches(&join.table, name) {
            return Ok(FromItem::Table(join.table.clone()));
        }
    }
    Err(PlanError::UnknownTable {
        name: name.to_string(),
    })
}

fn from_matches(from: &FromItem, name: &str) -> bool {
    match from {
        FromItem::Table(t) => table_matches(t, name),
        FromItem::Subquery { alias, .. } => alias.value.eq_ignore_ascii_case(name),
    }
}

fn table_matches(table: &TableRef, name: &str) -> bool {
    table.name.value.eq_ignore_ascii_case(name)
        || table
            .alias
            .as_ref()
            .is_some_and(|a| a.value.eq_ignore_ascii_case(name))
}
