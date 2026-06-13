//! HNSW-accelerated K-NN SELECT fast path.

use noedb_ast::{BinaryOp, ColumnRef, Expr, FromItem, Literal, SelectItem, SelectStmt, Statement};
use noedb_planner::{Record, Value};

use crate::engine::{LocalEngine, QueryResult};
use crate::error::EngineError;
use crate::machine::row_key;

struct KnnQuery<'a> {
    table: &'a str,
    vector_col: &'a str,
    query: Vec<f32>,
    limit: usize,
    items: &'a [SelectItem],
}

/// Run a vector top-k query via the in-memory HNSW index when the SQL matches.
pub(crate) fn try_hnsw_select(
    eng: &LocalEngine,
    stmt: &Statement,
) -> Result<Option<QueryResult>, EngineError> {
    let Statement::Select(select) = stmt else {
        return Ok(None);
    };
    let Some(spec) = detect_knn(select) else {
        return Ok(None);
    };
    let hits = eng
        .vector_search(spec.table, spec.vector_col, &spec.query, spec.limit);
    if hits.is_empty() {
        return Ok(None);
    }
    let tree = eng.storage().read();
    let records = hits
        .iter()
        .filter_map(|(row_id, _)| build_record(&tree, spec.table, row_id, spec.items))
        .collect::<Vec<_>>();
    if records.len() != hits.len() {
        return Ok(None);
    }
    Ok(Some(QueryResult::from_records(&records)))
}

fn detect_knn<'a>(select: &'a SelectStmt) -> Option<KnnQuery<'a>> {
    if select.distinct
        || !select.group_by.is_empty()
        || select.having_clause.is_some()
        || select.compound.is_some()
        || !select.joins.is_empty()
        || select.offset.is_some()
        || select.where_clause.is_some()
    {
        return None;
    }
    let limit = usize::try_from(select.limit?).ok()?;
    if limit == 0 || select.order_by.len() != 1 {
        return None;
    }
    let key = select.order_by.first()?;
    let (vector_col, query) = distance_spec(&key.expr)?;
    let FromItem::Table(table) = select.from.as_ref()? else {
        return None;
    };
    if !select.items.iter().all(is_simple_column) {
        return None;
    }
    Some(KnnQuery {
        table: &table.name.value,
        vector_col,
        query,
        limit,
        items: &select.items,
    })
}

fn is_simple_column(item: &SelectItem) -> bool {
    matches!(
        item.expr,
        Expr::Column(ColumnRef::Named {
            table: None,
            ..
        })
    )
}

fn distance_spec(expr: &Expr) -> Option<(&str, Vec<f32>)> {
    let Expr::Binary {
        op: BinaryOp::Distance,
        left,
        right,
        ..
    } = expr
    else {
        return None;
    };
    let col = match left.as_ref() {
        Expr::Column(ColumnRef::Named { column, table: None, .. }) => column.value.as_str(),
        _ => return None,
    };
    let query = literal_vector(right.as_ref())?;
    Some((col, query))
}

fn literal_vector(expr: &Expr) -> Option<Vec<f32>> {
    match expr {
        Expr::Literal(Literal::String(s, _)) => parse_vector_text(s).ok(),
        Expr::Literal(Literal::Float(f, _)) => Some(vec![*f as f32]),
        Expr::Literal(Literal::Integer(n, _)) => Some(vec![*n as f32]),
        _ => None,
    }
}

fn parse_vector_text(s: &str) -> Result<Vec<f32>, ()> {
    let s = s.trim().trim_start_matches('[').trim_end_matches(']');
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<f32>().map_err(|_| ()))
        .collect()
}

fn build_record(
    tree: &noedb_storage::LsmTree,
    table: &str,
    row_id: &str,
    items: &[SelectItem],
) -> Option<Record> {
    let mut fields = Vec::with_capacity(items.len());
    for item in items {
        let Expr::Column(ColumnRef::Named { column, .. }) = &item.expr else {
            return None;
        };
        let name = item
            .alias
            .as_ref()
            .map(|a| a.value.clone())
            .unwrap_or_else(|| column.value.clone());
        let key = row_key(table, row_id, &column.value);
        let bytes = tree.get(&key).ok()??;
        fields.push((name, Value::Bytes(bytes)));
    }
    Some(Record { fields })
}
