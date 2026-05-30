//! Expand `SELECT *` / `table.*` using the schema catalog before projection.

use noedb_ast::{ColumnRef, Expr, Ident, SelectItem, SelectStmt, TableRef};
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
    let mut items = expand_table_columns(from, schema, span, qualify)?;
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
    let table = resolve_table_ref(stmt, &qualifier.value)?;
    expand_table_columns(table, schema, span, qualify || !stmt.joins.is_empty())
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

fn resolve_table_ref<'a>(
    stmt: &'a SelectStmt,
    name: &str,
) -> Result<&'a TableRef, PlanError> {
    if let Some(from) = &stmt.from {
        if table_matches(from, name) {
            return Ok(from);
        }
    }
    for join in &stmt.joins {
        if table_matches(&join.table, name) {
            return Ok(&join.table);
        }
    }
    Err(PlanError::UnknownTable {
        name: name.to_string(),
    })
}

fn table_matches(table: &TableRef, name: &str) -> bool {
    table.name.value.eq_ignore_ascii_case(name)
        || table
            .alias
            .as_ref()
            .is_some_and(|a| a.value.eq_ignore_ascii_case(name))
}
