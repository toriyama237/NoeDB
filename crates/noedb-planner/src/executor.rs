//! Volcano-style executor (Weeks 18–23).

#![allow(
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::match_same_arms,
    clippy::option_if_let_else
)]

use std::collections::HashMap;

use noedb_ast::{Expr, SelectItem};
use noedb_storage::{LsmTree, StorageEngine, StorageError};

use crate::eval::{eval_expr, eval_predicate};
use crate::index::SecondaryIndex;
use crate::join::{column_name_matches, join_key_value};
use crate::logical::AggFunc;
use crate::physical::PhysicalPlan;
use crate::value::{Record, Value};
use crate::ExecError;

/// Storage-backed execution context.
pub struct ExecutionContext<'a, S: StorageEngine<Error = StorageError> = LsmTree> {
    /// Row/cell reads (may be MVCC snapshot store).
    pub store: &'a S,
    /// Base LSM for secondary index catalog lookups.
    pub index_catalog: &'a LsmTree,
    /// Materialized `WITH` CTE rows keyed by name (Week 40).
    pub cte_tables: Option<&'a HashMap<String, Vec<RowMap>>>,
}

impl<'a> ExecutionContext<'a, LsmTree> {
    /// Single-tree context (default path).
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn single(store: &'a LsmTree) -> Self {
        Self {
            store,
            index_catalog: store,
            cte_tables: None,
        }
    }
}

/// Pull-based row iterator over a physical plan.
pub struct Executor {
    state: ExecState,
}

enum ExecState {
    Done,
    LiteralProject {
        items: Vec<SelectItem>,
        emitted: bool,
    },
    SeqScan {
        rows: std::vec::IntoIter<RowMap>,
    },
    IndexScan {
        rows: std::vec::IntoIter<RowMap>,
    },
    Filter {
        predicate: Expr,
        child: Box<Executor>,
    },
    Project {
        items: Vec<SelectItem>,
        child: Box<Executor>,
    },
    HashJoin {
        rows: std::vec::IntoIter<RowMap>,
    },
    NestedLoopJoin {
        on: Expr,
        left: Vec<RowMap>,
        right: Vec<RowMap>,
        li: usize,
        ri: usize,
    },
    MergeJoin {
        pairs: std::vec::IntoIter<RowMap>,
    },
    Aggregate {
        groups: std::vec::IntoIter<(Vec<(String, Vec<u8>)>, Vec<(String, Value)>)>,
    },
    Sort {
        rows: std::vec::IntoIter<RowMap>,
    },
    Limit {
        limit: u64,
        child: Box<Executor>,
    },
    Window {
        rows: std::vec::IntoIter<RowMap>,
    },
    SemiJoin {
        rows: std::vec::IntoIter<RowMap>,
    },
    SetOp {
        rows: std::vec::IntoIter<RowMap>,
    },
    Dedup {
        child: Box<Executor>,
        seen: std::collections::HashSet<Vec<u8>>,
    },
}

pub(crate) type RowMap = Vec<(String, Value)>;

impl Executor {
    /// Build an executor for `plan`.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new<S: StorageEngine<Error = StorageError>>(
        plan: PhysicalPlan,
        ctx: &ExecutionContext<'_, S>,
    ) -> Result<Self, ExecError> {
        let state = build_state(plan, ctx)?;
        Ok(Self { state })
    }

    /// Pull the next result row.
    #[allow(clippy::never_loop)]
    pub fn next_row(&mut self) -> Result<Option<Record>, ExecError> {
        loop {
            match &mut self.state {
                ExecState::Done => return Ok(None),
                ExecState::LiteralProject { items, emitted } => {
                    if *emitted {
                        self.state = ExecState::Done;
                        return Ok(None);
                    }
                    *emitted = true;
                    let cells = items
                        .iter()
                        .map(|item| eval_expr(&item.expr, &[]))
                        .collect::<Result<Vec<_>, _>>()?;
                    return Ok(Some(record_from_cells(items, cells)));
                }
                ExecState::SeqScan { rows } | ExecState::IndexScan { rows } => {
                    let Some(row) = rows.next() else {
                        self.state = ExecState::Done;
                        return Ok(None);
                    };
                    return Ok(Some(Record { fields: row }));
                }
                ExecState::Filter { predicate, child } => {
                    while let Some(rec) = child.next_row()? {
                        if eval_predicate(predicate, &rec.fields)? {
                            return Ok(Some(rec));
                        }
                    }
                    self.state = ExecState::Done;
                    return Ok(None);
                }
                ExecState::Project { items, child } => {
                    while let Some(rec) = child.next_row()? {
                        let cells = items
                            .iter()
                            .map(|item| eval_expr(&item.expr, &rec.fields))
                            .collect::<Result<Vec<_>, _>>()?;
                        return Ok(Some(record_from_cells(items, cells)));
                    }
                    self.state = ExecState::Done;
                    return Ok(None);
                }
                ExecState::HashJoin { rows } => {
                    let Some(row) = rows.next() else {
                        self.state = ExecState::Done;
                        return Ok(None);
                    };
                    return Ok(Some(Record { fields: row }));
                }
                ExecState::NestedLoopJoin {
                    on,
                    left,
                    right,
                    li,
                    ri,
                } => {
                    while *li < left.len() {
                        while *ri < right.len() {
                            let merged = merge_rows(&left[*li], &right[*ri]);
                            *ri += 1;
                            if eval_predicate(on, &merged)? {
                                return Ok(Some(Record { fields: merged }));
                            }
                        }
                        *li += 1;
                        *ri = 0;
                    }
                    self.state = ExecState::Done;
                    return Ok(None);
                }
                ExecState::MergeJoin { pairs, .. } => {
                    let Some(row) = pairs.next() else {
                        self.state = ExecState::Done;
                        return Ok(None);
                    };
                    return Ok(Some(Record { fields: row }));
                }
                ExecState::Aggregate { groups, .. } => {
                    let Some((_, fields)) = groups.next() else {
                        self.state = ExecState::Done;
                        return Ok(None);
                    };
                    return Ok(Some(Record { fields }));
                }
                ExecState::Sort { rows } => {
                    let Some(row) = rows.next() else {
                        self.state = ExecState::Done;
                        return Ok(None);
                    };
                    return Ok(Some(Record { fields: row }));
                }
                ExecState::Window { rows }
                | ExecState::SemiJoin { rows }
                | ExecState::SetOp { rows } => {
                    let Some(row) = rows.next() else {
                        self.state = ExecState::Done;
                        return Ok(None);
                    };
                    return Ok(Some(Record { fields: row }));
                }
                ExecState::Dedup { child, seen } => {
                    while let Some(rec) = child.next_row()? {
                        let key = crate::setops::row_key(&rec.fields);
                        if seen.insert(key) {
                            return Ok(Some(rec));
                        }
                    }
                    self.state = ExecState::Done;
                    return Ok(None);
                }
                ExecState::Limit { limit, child } => {
                    if *limit == 0 {
                        self.state = ExecState::Done;
                        return Ok(None);
                    }
                    if let Some(rec) = child.next_row()? {
                        *limit -= 1;
                        return Ok(Some(rec));
                    }
                    self.state = ExecState::Done;
                    return Ok(None);
                }
            }
        }
    }

    /// Collect all rows (convenience for tests).
    pub fn collect(mut self) -> Result<Vec<Record>, ExecError> {
        let mut out = Vec::new();
        while let Some(row) = self.next_row()? {
            out.push(row);
        }
        Ok(out)
    }
}

#[allow(clippy::unnecessary_wraps)]
fn build_state<S: StorageEngine<Error = StorageError>>(
    plan: PhysicalPlan,
    ctx: &ExecutionContext<'_, S>,
) -> Result<ExecState, ExecError> {
    match plan {
        PhysicalPlan::SeqScan {
            table, columns: _, ..
        } if table.is_empty() => Ok(ExecState::LiteralProject {
            items: vec![],
            emitted: false,
        }),
        PhysicalPlan::SeqScan {
            table,
            prefix,
            columns,
        } => {
            let storage_cols = columns.as_ref().map(|cols| {
                cols.iter()
                    .map(|c| {
                        c.rsplit_once('.')
                            .map_or_else(|| c.clone(), |(_, bare)| bare.to_string())
                    })
                    .collect::<Vec<_>>()
            });
            let rows = prefix_rows(
                load_table_rows(ctx.store, &table, storage_cols.as_deref()),
                &prefix,
            );
            Ok(ExecState::SeqScan {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::CteScan {
            name,
            prefix,
            columns,
        } => {
            let rows = ctx
                .cte_tables
                .and_then(|t| t.get(&name))
                .cloned()
                .unwrap_or_default();
            let rows = prefix_rows(rows, &prefix);
            let rows = prune_cte_rows(&rows, columns.as_deref());
            Ok(ExecState::SeqScan {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::SubqueryScan { input, prefix, .. } => {
            let rows = execute_to_rows(*input, ctx)?;
            let rows = prefix_rows(rows, &prefix);
            Ok(ExecState::SeqScan {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::IndexScan {
            table,
            column,
            point_key,
            columns,
        } => {
            let row_ids = match point_key {
                Some(key) => SecondaryIndex::lookup(ctx.index_catalog, &table, &column, &key),
                None => SecondaryIndex::load(ctx.index_catalog, &table, &column)
                    .iter()
                    .flat_map(|(_, ids)| ids.clone())
                    .collect(),
            };
            let mut rows = Vec::new();
            for row_id in row_ids {
                if let Some(row) = load_row_by_id(ctx.store, &table, &row_id, columns.as_deref()) {
                    rows.push(row);
                }
            }
            Ok(ExecState::IndexScan {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::Filter { input, predicate } => {
            let child = Executor::new(*input, ctx)?;
            Ok(ExecState::Filter {
                predicate,
                child: Box::new(child),
            })
        }
        PhysicalPlan::Project { input, items } => {
            if matches!(
                &*input,
                PhysicalPlan::SeqScan { table, .. } if table.is_empty()
            ) {
                return Ok(ExecState::LiteralProject {
                    items,
                    emitted: false,
                });
            }
            let child = Executor::new(*input, ctx)?;
            Ok(ExecState::Project {
                items,
                child: Box::new(child),
            })
        }
        PhysicalPlan::HashJoin {
            left,
            right,
            left_key,
            right_key,
            left_outer,
            ..
        } => {
            let left_rows = execute_to_rows(*left, ctx)?;
            let right_rows = execute_to_rows(*right, ctx)?;
            // For LEFT joins, gather the right side's column names so unmatched
            // left rows can be padded with NULLs (a projection of a missing
            // column would otherwise error rather than yield NULL).
            let right_cols: Vec<String> = if left_outer {
                let mut cols: Vec<String> = Vec::new();
                for row in &right_rows {
                    for (name, _) in row {
                        if !cols.iter().any(|c| c == name) {
                            cols.push(name.clone());
                        }
                    }
                }
                cols
            } else {
                Vec::new()
            };
            let mut buckets: HashMap<Vec<u8>, Vec<RowMap>> = HashMap::new();
            for row in right_rows {
                if let Some(key) = join_key_value(&row, &right_key) {
                    buckets.entry(key).or_default().push(row);
                }
            }
            let mut out = Vec::new();
            for lrow in left_rows {
                let matches = join_key_value(&lrow, &left_key).and_then(|key| buckets.get(&key));
                match matches {
                    Some(matches) if !matches.is_empty() => {
                        for rrow in matches {
                            out.push(merge_rows(&lrow, rrow));
                        }
                    }
                    _ if left_outer => {
                        let mut row = lrow;
                        for col in &right_cols {
                            if !row.iter().any(|(n, _)| n == col) {
                                row.push((col.clone(), Value::Null));
                            }
                        }
                        out.push(row);
                    }
                    _ => {}
                }
            }
            Ok(ExecState::HashJoin {
                rows: out.into_iter(),
            })
        }
        PhysicalPlan::NestedLoopJoin { left, right, on } => {
            let left_rows = execute_to_rows(*left, ctx)?;
            let right_rows = execute_to_rows(*right, ctx)?;
            Ok(ExecState::NestedLoopJoin {
                on,
                left: left_rows,
                right: right_rows,
                li: 0,
                ri: 0,
            })
        }
        PhysicalPlan::MergeJoin {
            left,
            right,
            on,
            left_key,
            right_key,
        } => {
            let mut left_rows = execute_to_rows(*left, ctx)?;
            let mut right_rows = execute_to_rows(*right, ctx)?;
            left_rows.sort_by_key(|row| join_key_value(row, &left_key));
            right_rows.sort_by_key(|row| join_key_value(row, &right_key));
            let mut pairs = Vec::new();
            let mut i = 0;
            let mut j = 0;
            while i < left_rows.len() && j < right_rows.len() {
                let lk = join_key_value(&left_rows[i], &left_key);
                let rk = join_key_value(&right_rows[j], &right_key);
                match lk.cmp(&rk) {
                    std::cmp::Ordering::Less => i += 1,
                    std::cmp::Ordering::Greater => j += 1,
                    std::cmp::Ordering::Equal => {
                        let merged = merge_rows(&left_rows[i], &right_rows[j]);
                        if eval_predicate(&on, &merged)? {
                            pairs.push(merged);
                        }
                        j += 1;
                    }
                }
            }
            Ok(ExecState::MergeJoin {
                pairs: pairs.into_iter(),
            })
        }
        PhysicalPlan::Aggregate {
            input,
            group_by,
            aggs,
        } => {
            let rows = execute_to_rows(*input, ctx)?;
            let mut groups: HashMap<Vec<(String, Vec<u8>)>, HashMap<String, AggSlot>> =
                HashMap::new();
            for row in rows {
                let key: Vec<(String, Vec<u8>)> = group_by
                    .iter()
                    .map(|col| {
                        let val = row_value(&row, col);
                        (col.clone(), val.as_bytes())
                    })
                    .collect();
                let entry = groups.entry(key).or_default();
                for (name, func) in &aggs {
                    let slot = entry.entry(name.clone()).or_default();
                    match func {
                        AggFunc::CountStar => slot.count_star += 1,
                        AggFunc::CountCol(col) => {
                            if !matches!(row_value(&row, col), Value::Null) {
                                slot.count_col += 1;
                            }
                        }
                        AggFunc::Sum(col) | AggFunc::Avg(col) => {
                            if let Some(n) = numeric_value(&row_value(&row, col)) {
                                slot.sum += n;
                                slot.sum_count += 1;
                            }
                        }
                        AggFunc::Min(col) => {
                            if let Some(n) = numeric_value(&row_value(&row, col)) {
                                slot.min = Some(slot.min.map_or(n, |m| m.min(n)));
                            }
                        }
                        AggFunc::Max(col) => {
                            if let Some(n) = numeric_value(&row_value(&row, col)) {
                                slot.max = Some(slot.max.map_or(n, |m| m.max(n)));
                            }
                        }
                    }
                }
            }
            #[allow(clippy::needless_collect, clippy::type_complexity)]
            let out: Vec<(Vec<(String, Vec<u8>)>, Vec<(String, Value)>)> = groups
                .into_iter()
                .map(|(gkey, slots)| {
                    let mut fields = gkey
                        .into_iter()
                        .map(|(n, bytes)| (n, Value::Bytes(bytes)))
                        .collect::<Vec<_>>();
                    for (name, slot) in slots {
                        if let Some((_, func)) = aggs.iter().find(|(n, _)| n == &name) {
                            fields.push((name, slot.finish(func)));
                        }
                    }
                    (Vec::new(), fields)
                })
                .collect();
            Ok(ExecState::Aggregate {
                groups: out.into_iter(),
            })
        }
        PhysicalPlan::Sort { input, keys, top_k } => {
            let mut rows = execute_to_rows(*input, ctx)?;
            if let Some(k) = top_k {
                let k = k as usize;
                if k > 0 && rows.len() > k {
                    rows.select_nth_unstable_by(k - 1, |a, b| compare_rows(a, b, &keys));
                    rows.truncate(k);
                }
            }
            rows.sort_by(|a, b| compare_rows(a, b, &keys));
            Ok(ExecState::Sort {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::Limit {
            input,
            limit,
            offset,
        } => {
            let mut child = Executor::new(*input, ctx)?;
            let mut skipped = 0u64;
            while skipped < offset {
                if child.next_row()?.is_none() {
                    return Ok(ExecState::Done);
                }
                skipped += 1;
            }
            Ok(ExecState::Limit {
                limit,
                child: Box::new(child),
            })
        }
        PhysicalPlan::Window { input, windows } => {
            let rows = execute_to_rows(*input, ctx)?;
            let rows = crate::window_exec::apply_windows(rows, &windows)?;
            Ok(ExecState::Window {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::SemiJoin {
            left,
            right,
            left_key,
            right_key,
            corr_on,
            negated,
        } => {
            let left_rows = execute_to_rows(*left, ctx)?;
            let right_rows = execute_to_rows(*right, ctx)?;
            let rows = semi_join_rows(
                left_rows,
                &right_rows,
                &left_key,
                &right_key,
                corr_on.as_ref(),
                negated,
            )?;
            Ok(ExecState::SemiJoin {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::SetOp {
            left,
            right,
            op,
            all,
        } => {
            let left_rows = execute_to_rows(*left, ctx)?;
            let right_rows = execute_to_rows(*right, ctx)?;
            let rows = crate::setops::combine_set_op(left_rows, &right_rows, op, all);
            Ok(ExecState::SetOp {
                rows: rows.into_iter(),
            })
        }
        PhysicalPlan::Dedup { input } => Ok(ExecState::Dedup {
            child: Box::new(Executor::new(*input, ctx)?),
            seen: std::collections::HashSet::new(),
        }),
    }
}

fn semi_join_rows(
    left_rows: Vec<RowMap>,
    right_rows: &[RowMap],
    left_key: &str,
    right_key: &str,
    corr_on: Option<&Expr>,
    negated: bool,
) -> Result<Vec<RowMap>, ExecError> {
    if let Some(corr_on) = corr_on {
        semi_join_rows_hash_correlated(left_rows, right_rows, left_key, right_key, corr_on, negated)
    } else {
        semi_join_rows_hash(left_rows, right_rows, left_key, right_key, negated)
    }
}

fn semi_join_rows_hash(
    left_rows: Vec<RowMap>,
    right_rows: &[RowMap],
    left_key: &str,
    right_key: &str,
    negated: bool,
) -> Result<Vec<RowMap>, ExecError> {
    use std::collections::HashSet;

    let mut right_keys = HashSet::new();
    for rrow in right_rows {
        if let Some(rkey) = join_key_value(rrow, right_key) {
            right_keys.insert(rkey);
        }
    }

    let mut out = Vec::new();
    for lrow in left_rows {
        let Some(lkey) = join_key_value(&lrow, left_key) else {
            continue;
        };
        let member = right_keys.contains(&lkey);
        if negated {
            if !member {
                out.push(lrow);
            }
        } else if member {
            out.push(lrow);
        }
    }
    Ok(out)
}

fn semi_join_rows_hash_correlated(
    left_rows: Vec<RowMap>,
    right_rows: &[RowMap],
    left_key: &str,
    right_key: &str,
    corr_on: &Expr,
    negated: bool,
) -> Result<Vec<RowMap>, ExecError> {
    use std::collections::HashMap;

    let mut buckets: HashMap<Vec<u8>, Vec<&RowMap>> = HashMap::new();
    for rrow in right_rows {
        if let Some(rkey) = join_key_value(rrow, right_key) {
            buckets.entry(rkey).or_default().push(rrow);
        }
    }

    let mut out = Vec::new();
    for lrow in left_rows {
        let Some(lkey) = join_key_value(&lrow, left_key) else {
            continue;
        };
        let mut matched = false;
        if let Some(candidates) = buckets.get(&lkey) {
            for rrow in candidates {
                let merged = merge_rows(&lrow, rrow);
                if eval_predicate(corr_on, &merged)? {
                    matched = true;
                    break;
                }
            }
        }
        if negated {
            if !matched {
                out.push(lrow);
            }
        } else if matched {
            out.push(lrow);
        }
    }
    Ok(out)
}

fn execute_to_rows<S: StorageEngine<Error = StorageError>>(
    plan: PhysicalPlan,
    ctx: &ExecutionContext<'_, S>,
) -> Result<Vec<RowMap>, ExecError> {
    Executor::new(plan, ctx)?
        .collect()
        .map(|recs| recs.into_iter().map(|r| r.fields).collect())
}

#[allow(clippy::option_if_let_else)]
fn record_from_cells(items: &[SelectItem], cells: Vec<Value>) -> Record {
    let fields = items
        .iter()
        .zip(cells)
        .map(|(item, v)| {
            let name = if let Some(alias) = &item.alias {
                alias.value.clone()
            } else if let Expr::Column(noedb_ast::ColumnRef::Named { column, .. }) = &item.expr {
                column.value.clone()
            } else {
                "col".into()
            };
            (name, v)
        })
        .collect();
    Record { fields }
}

#[derive(Default)]
struct AggSlot {
    count_star: u64,
    count_col: u64,
    sum: f64,
    sum_count: u64,
    min: Option<f64>,
    max: Option<f64>,
}

impl AggSlot {
    fn finish(self, func: &AggFunc) -> Value {
        match func {
            AggFunc::CountStar => {
                Value::Integer(i64::try_from(self.count_star).unwrap_or(i64::MAX))
            }
            AggFunc::CountCol(_) => {
                Value::Integer(i64::try_from(self.count_col).unwrap_or(i64::MAX))
            }
            AggFunc::Sum(_) => Value::Float(self.sum),
            AggFunc::Avg(_) => {
                if self.sum_count == 0 {
                    Value::Null
                } else {
                    Value::Float(self.sum / self.sum_count as f64)
                }
            }
            AggFunc::Min(_) => self.min.map(Value::Float).unwrap_or(Value::Null),
            AggFunc::Max(_) => self.max.map(Value::Float).unwrap_or(Value::Null),
        }
    }
}

fn row_value(row: &RowMap, col: &str) -> Value {
    row.iter()
        .find(|(n, _)| column_name_matches(n, col))
        .map_or(Value::Null, |(_, v)| v.clone())
}

fn numeric_value(v: &Value) -> Option<f64> {
    match v {
        Value::Null => None,
        Value::Integer(n) => Some(*n as f64),
        Value::Float(f) => Some(*f),
        Value::Bytes(b) => std::str::from_utf8(b).ok()?.trim().parse().ok(),
        _ => None,
    }
}

fn merge_rows(left: &RowMap, right: &RowMap) -> RowMap {
    let mut out = left.clone();
    out.extend(right.clone());
    out
}

pub(crate) fn compare_rows(a: &RowMap, b: &RowMap, keys: &[(String, bool)]) -> std::cmp::Ordering {
    for (col, asc) in keys {
        let va = a
            .iter()
            .find(|(n, _)| column_name_matches(n, col))
            .map(|(_, v)| v);
        let vb = b
            .iter()
            .find(|(n, _)| column_name_matches(n, col))
            .map(|(_, v)| v);
        let ord = compare_sort_values(va, vb);
        if ord != std::cmp::Ordering::Equal {
            return if *asc { ord } else { ord.reverse() };
        }
    }
    std::cmp::Ordering::Equal
}

/// SQL `ORDER BY` with `NULLS LAST` (NULL sorts after all values).
fn compare_sort_values(a: Option<&Value>, b: Option<&Value>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) | (Some(Value::Null), Some(Value::Null)) => std::cmp::Ordering::Equal,
        (None, Some(_)) | (Some(Value::Null), Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) | (Some(_), Some(Value::Null)) => std::cmp::Ordering::Less,
        (Some(va), Some(vb)) => va.as_bytes().cmp(&vb.as_bytes()),
    }
}

/// Row keys: `table\0row_id\0column` → cell value; collapsed to one row per `row_id`.
fn load_table_rows<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    columns: Option<&[String]>,
) -> Vec<RowMap> {
    crate::parallel::load_table_rows(store, table, columns)
}

fn load_row_by_id<S: StorageEngine<Error = StorageError>>(
    store: &S,
    table: &str,
    row_id: &[u8],
    columns: Option<&[String]>,
) -> Option<RowMap> {
    if let Some(cols) = columns {
        let mut row = RowMap::new();
        for col in cols {
            let key = table_cell_key(table, row_id, col.as_bytes());
            if let Ok(Some(val)) = store.get(&key) {
                row.push((col.clone(), Value::Bytes(val)));
            }
        }
        return if row.is_empty() { None } else { Some(row) };
    }

    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);
    prefix.extend_from_slice(row_id);
    prefix.push(0);
    let end = noedb_storage::prefix_end(&prefix);

    let mut row = RowMap::new();
    for (key, val) in store.range(&prefix, &end) {
        if !key.starts_with(&prefix) {
            continue;
        }
        let col = &key[prefix.len()..];
        let col_name = String::from_utf8_lossy(col).into_owned();
        row.push((col_name, Value::Bytes(val)));
    }
    if row.is_empty() {
        None
    } else {
        Some(row)
    }
}

fn prefix_rows(rows: Vec<RowMap>, prefix: &str) -> Vec<RowMap> {
    if prefix.is_empty() {
        return rows;
    }
    rows.into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(n, v)| (format!("{prefix}.{n}"), v))
                .collect()
        })
        .collect()
}

fn prune_cte_rows(rows: &[RowMap], columns: Option<&[String]>) -> Vec<RowMap> {
    let Some(cols) = columns else {
        return rows.to_vec();
    };
    rows.iter()
        .map(|row| {
            cols.iter()
                .filter_map(|c| {
                    row.iter()
                        .find(|(n, _)| column_name_matches(n, c))
                        .map(|(n, v)| (n.clone(), v.clone()))
                })
                .collect()
        })
        .collect()
}

fn table_cell_key(table: &str, row_id: &[u8], column: &[u8]) -> Vec<u8> {
    let mut key = table.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(row_id);
    key.push(0);
    key.extend_from_slice(column);
    key
}

/// Execute a physical plan and return all rows.
pub fn execute<S: StorageEngine<Error = StorageError>>(
    plan: PhysicalPlan,
    ctx: &ExecutionContext<'_, S>,
) -> Result<Vec<Record>, ExecError> {
    Executor::new(plan, ctx)?.collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod join_tests {
    use super::*;
    use noedb_storage::{LsmConfig, LsmTree};

    fn temp_tree() -> (LsmTree, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "noedb-exec-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (LsmTree::open(&dir, LsmConfig::default()).unwrap(), dir)
    }

    fn put_row(tree: &mut LsmTree, table: &str, row: &str, col: &str, val: &[u8]) {
        let mut key = table.as_bytes().to_vec();
        key.push(0);
        key.extend_from_slice(row.as_bytes());
        key.push(0);
        key.extend_from_slice(col.as_bytes());
        tree.put(&key, val).unwrap();
    }

    #[test]
    fn column_name_matches_bare_and_qualified() {
        assert!(column_name_matches("hc.ag.country", "country"));
        assert!(column_name_matches("hc.n", "n"));
        assert!(column_name_matches("hc.ag.country", "hc.country"));
        assert!(!column_name_matches("hc.ag.country", "region"));
    }

    #[test]
    fn prune_cte_rows_resolves_bare_names() {
        let rows = vec![vec![
            ("hc.ag.country".into(), Value::Bytes(b"FR".to_vec())),
            ("hc.n".into(), Value::Integer(2)),
        ]];
        let pruned = prune_cte_rows(&rows, Some(&["country".into(), "n".into()]));
        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].len(), 2);
        assert_eq!(pruned[0][0].0, "hc.ag.country");
        assert_eq!(pruned[0][1].0, "hc.n");
    }

    #[test]
    fn hash_join_executes() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"1");
        put_row(&mut tree, "users", "1", "name", b"ada");
        put_row(&mut tree, "orders", "10", "user_id", b"1");
        put_row(&mut tree, "orders", "10", "amount", b"99");

        let on =
            noedb_parser::parse("SELECT 1 FROM users u INNER JOIN orders o ON u.id = o.user_id")
                .unwrap();
        let on_expr = match on {
            noedb_ast::Statement::Select(s) => s.joins[0].on.clone(),
            _ => panic!("select"),
        };
        let keys = crate::join::extract_equi_join(&on_expr).unwrap();
        let ctx = ExecutionContext::single(&tree);
        let plan = PhysicalPlan::HashJoin {
            left: Box::new(PhysicalPlan::SeqScan {
                table: "users".into(),
                prefix: "u".into(),
                columns: None,
            }),
            right: Box::new(PhysicalPlan::SeqScan {
                table: "orders".into(),
                prefix: "o".into(),
                columns: None,
            }),
            on: on_expr,
            left_key: keys.left,
            right_key: keys.right,
            left_outer: false,
        };
        let rows = execute(plan, &ctx).unwrap();
        assert_eq!(rows.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
