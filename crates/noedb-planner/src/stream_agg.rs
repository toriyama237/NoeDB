//! Streaming aggregation over table scans (v2.4).
//!
//! The generic path materializes every row of the table as a
//! `Vec<(String, Value)>` before aggregating — for `COUNT(*)` over 50 k
//! rows that is 50 k allocations of names and values that are thrown
//! away immediately. This module recognizes `Aggregate(SeqScan)` and
//! `Aggregate(Filter(SeqScan))` shapes and computes the aggregates in
//! one streaming pass over the storage range:
//!
//! - packed records are decoded **zero-copy** ([`noedb_storage::iter_row`]);
//! - only the columns referenced by the aggregates, the `GROUP BY` and
//!   the predicate are extracted (a bare `COUNT(*)` extracts nothing);
//! - predicates are evaluated with the same [`eval_predicate`] as the
//!   generic path, so semantics cannot drift.

use std::collections::HashMap;

use noedb_ast::{ColumnRef, Expr};
use noedb_storage::{prefix_end, StorageEngine, StorageError};

use crate::eval::eval_predicate;
use crate::executor::{AggSlot, ExecutionContext, RowMap};
use crate::logical::AggFunc;
use crate::physical::PhysicalPlan;
use crate::value::Value;
use crate::ExecError;

/// Aggregate groups in the executor's output shape.
pub(crate) type AggGroups = Vec<(Vec<(String, Vec<u8>)>, Vec<(String, Value)>)>;

/// Try to compute `Aggregate { input, group_by, aggs }` in one streaming
/// pass. Returns `None` when the plan shape is not eligible (the caller
/// falls back to the generic materializing path).
pub(crate) fn try_streaming_aggregate<S: StorageEngine<Error = StorageError>>(
    input: &PhysicalPlan,
    group_by: &[String],
    aggs: &[(String, AggFunc)],
    ctx: &ExecutionContext<'_, S>,
) -> Option<Result<AggGroups, ExecError>> {
    let (table, predicate) = match input {
        PhysicalPlan::SeqScan { table, .. } if !table.is_empty() => (table.as_str(), None),
        PhysicalPlan::Filter { input, predicate } => match &**input {
            PhysicalPlan::SeqScan { table, .. } if !table.is_empty() => {
                if !expr_is_streamable(predicate) {
                    return None;
                }
                (table.as_str(), Some(predicate))
            }
            _ => return None,
        },
        _ => return None,
    };

    // Columns the pass actually needs (bare names; storage stores bare).
    let mut needed: Vec<String> = Vec::new();
    let mut need = |name: &str| {
        let bare = name.rsplit_once('.').map_or(name, |(_, b)| b);
        if !needed.iter().any(|n| n == bare) {
            needed.push(bare.to_string());
        }
    };
    for col in group_by {
        need(col);
    }
    for (_, func) in aggs {
        match func {
            AggFunc::CountStar => {}
            AggFunc::CountCol(c)
            | AggFunc::Sum(c)
            | AggFunc::Avg(c)
            | AggFunc::Min(c)
            | AggFunc::Max(c) => need(c),
        }
    }
    if let Some(pred) = predicate {
        for col in predicate_columns(pred) {
            need(&col);
        }
    }

    Some(run(ctx.store, table, predicate, group_by, aggs, &needed))
}

/// One streaming pass: scan the table range, assemble each row's needed
/// columns, filter, and fold into aggregate slots.
fn run<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    predicate: Option<&Expr>,
    group_by: &[String],
    aggs: &[(String, AggFunc)],
    needed: &[String],
) -> Result<AggGroups, ExecError> {
    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);
    let end = prefix_end(&prefix);

    // Pure `COUNT(*)`: no column, no predicate, no grouping — count
    // distinct row ids over a key-only scan (no value is ever copied).
    if needed.is_empty() && predicate.is_none() && group_by.is_empty() {
        let mut rows: u64 = 0;
        let mut last_id: Option<Vec<u8>> = None;
        for key in store.range_keys(&prefix, &end) {
            if !key.starts_with(&prefix) {
                continue;
            }
            let rest = &key[prefix.len()..];
            let row_id = rest
                .iter()
                .position(|&b| b == 0)
                .map_or(rest, |pos| &rest[..pos]);
            if last_id.as_deref() != Some(row_id) {
                rows += 1;
                last_id = Some(row_id.to_vec());
            }
        }
        let mut slot = AggSlot::default();
        slot.count_star = rows;
        let fields: Vec<(String, Value)> = aggs
            .iter()
            .map(|(name, func)| (name.clone(), slot.clone().finish(func)))
            .collect();
        return Ok(vec![(Vec::new(), fields)]);
    }

    let mut groups: HashMap<Vec<(String, Vec<u8>)>, Vec<AggSlot>> = HashMap::new();
    let mut current_id: Option<Vec<u8>> = None;
    // Reused buffer: needed columns of the row being assembled.
    let mut row: RowMap = Vec::with_capacity(needed.len());

    let mut flush = |row: &mut RowMap| -> Result<(), ExecError> {
        let keep = match predicate {
            Some(pred) => eval_predicate(pred, row)?,
            None => true,
        };
        if keep {
            fold_row(&mut groups, row, group_by, aggs);
        }
        row.clear();
        Ok(())
    };

    for (key, val) in store.range(&prefix, &end) {
        if !key.starts_with(&prefix) {
            continue;
        }
        let rest = &key[prefix.len()..];
        let (row_id, cell_col) = match rest.iter().position(|&b| b == 0) {
            Some(pos) => (&rest[..pos], Some(&rest[pos + 1..])),
            None => (rest, None),
        };
        if current_id.as_deref() != Some(row_id) {
            if current_id.is_some() {
                flush(&mut row)?;
            }
            current_id = Some(row_id.to_vec());
        }
        match cell_col {
            None => {
                if noedb_storage::is_packed_row(&val) {
                    // Zero-copy walk; copy only the needed columns.
                    for (name, bytes) in noedb_storage::iter_row(&val) {
                        if needed.iter().any(|n| n == name) {
                            upsert(&mut row, name, bytes);
                        }
                    }
                } else if needed.iter().any(|n| n == "value") {
                    upsert(&mut row, "value", &val);
                }
            }
            Some(col) => {
                let Ok(name) = core::str::from_utf8(col) else {
                    continue;
                };
                if needed.iter().any(|n| n == name) {
                    upsert(&mut row, name, &val);
                }
            }
        }
    }
    if current_id.is_some() {
        flush(&mut row)?;
    }

    // SQL semantics: a global aggregate over an empty input yields one row.
    if groups.is_empty() && group_by.is_empty() {
        groups.insert(Vec::new(), vec![AggSlot::default(); aggs.len()]);
    }

    Ok(groups
        .into_iter()
        .map(|(gkey, slots)| {
            let mut fields: Vec<(String, Value)> = gkey
                .iter()
                .map(|(n, bytes)| (n.clone(), Value::Bytes(bytes.clone())))
                .collect();
            for ((name, func), slot) in aggs.iter().zip(slots) {
                fields.push((name.clone(), slot.finish(func)));
            }
            (gkey, fields)
        })
        .collect())
}

/// Later writes win: cells override packed columns (same rule as scans).
fn upsert(row: &mut RowMap, name: &str, bytes: &[u8]) {
    if let Some(slot) = row.iter_mut().find(|(n, _)| n == name) {
        slot.1 = Value::Bytes(bytes.to_vec());
    } else {
        row.push((name.to_string(), Value::Bytes(bytes.to_vec())));
    }
}

fn fold_row(
    groups: &mut HashMap<Vec<(String, Vec<u8>)>, Vec<AggSlot>>,
    row: &RowMap,
    group_by: &[String],
    aggs: &[(String, AggFunc)],
) {
    let key: Vec<(String, Vec<u8>)> = group_by
        .iter()
        .map(|col| (col.clone(), crate::executor::row_value(row, col).as_bytes()))
        .collect();
    let slots = groups
        .entry(key)
        .or_insert_with(|| vec![AggSlot::default(); aggs.len()]);
    for ((_, func), slot) in aggs.iter().zip(slots.iter_mut()) {
        slot.update(func, row);
    }
}

/// Whether the fast path can evaluate this predicate: plain expressions
/// only — no subqueries (they need their own executor context).
fn expr_is_streamable(expr: &Expr) -> bool {
    match expr {
        Expr::Literal(_) => true,
        Expr::Column(ColumnRef::Named { .. }) => true,
        Expr::Column(_) => false,
        Expr::Binary { left, right, .. } => expr_is_streamable(left) && expr_is_streamable(right),
        Expr::Unary { expr, .. } | Expr::IsNull { expr, .. } | Expr::Cast { expr, .. } => {
            expr_is_streamable(expr)
        }
        Expr::Paren(inner, _) => expr_is_streamable(inner),
        Expr::In { expr, values, .. } => {
            expr_is_streamable(expr) && values.iter().all(expr_is_streamable)
        }
        Expr::Between {
            expr, low, high, ..
        } => expr_is_streamable(expr) && expr_is_streamable(low) && expr_is_streamable(high),
        Expr::Function { args, over, .. } => over.is_none() && args.iter().all(expr_is_streamable),
        _ => false,
    }
}

/// Columns referenced anywhere in a streamable predicate.
fn predicate_columns(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_predicate_columns(expr, &mut out);
    out
}

fn collect_predicate_columns(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Column(ColumnRef::Named { table, column }) => {
            let name = table.as_ref().map_or_else(
                || column.value.clone(),
                |t| format!("{}.{}", t.value, column.value),
            );
            out.push(name);
        }
        Expr::Binary { left, right, .. } => {
            collect_predicate_columns(left, out);
            collect_predicate_columns(right, out);
        }
        Expr::Unary { expr, .. } | Expr::IsNull { expr, .. } | Expr::Cast { expr, .. } => {
            collect_predicate_columns(expr, out);
        }
        Expr::Paren(inner, _) => collect_predicate_columns(inner, out),
        Expr::In { expr, values, .. } => {
            collect_predicate_columns(expr, out);
            for v in values {
                collect_predicate_columns(v, out);
            }
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_predicate_columns(expr, out);
            collect_predicate_columns(low, out);
            collect_predicate_columns(high, out);
        }
        Expr::Function { args, .. } => {
            for a in args {
                collect_predicate_columns(a, out);
            }
        }
        _ => {}
    }
}
