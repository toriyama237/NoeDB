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
use crate::join::join_key_value;
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
}

impl<'a> ExecutionContext<'a, LsmTree> {
    /// Single-tree context (default path).
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn single(store: &'a LsmTree) -> Self {
        Self {
            store,
            index_catalog: store,
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
}

type RowMap = Vec<(String, Value)>;

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
        PhysicalPlan::SeqScan { table, columns: _ } if table.is_empty() => {
            Ok(ExecState::LiteralProject {
                items: vec![],
                emitted: false,
            })
        }
        PhysicalPlan::SeqScan { table, columns } => {
            let rows = load_table_rows(ctx.store, &table, columns.as_deref());
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
            ..
        } => {
            let left_rows = execute_to_rows(*left, ctx)?;
            let right_rows = execute_to_rows(*right, ctx)?;
            let mut buckets: HashMap<Vec<u8>, Vec<RowMap>> = HashMap::new();
            for row in right_rows {
                if let Some(key) = join_key_value(&row, &right_key) {
                    buckets.entry(key).or_default().push(row);
                }
            }
            let mut out = Vec::new();
            for lrow in left_rows {
                let Some(key) = join_key_value(&lrow, &left_key) else {
                    continue;
                };
                let Some(matches) = buckets.get(&key) else {
                    continue;
                };
                for rrow in matches {
                    out.push(merge_rows(&lrow, rrow));
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
            let mut groups: HashMap<Vec<(String, Vec<u8>)>, HashMap<String, u64>> = HashMap::new();
            for row in rows {
                let key: Vec<(String, Vec<u8>)> = group_by
                    .iter()
                    .map(|col| {
                        let val = row
                            .iter()
                            .find(|(n, _)| n == col)
                            .map_or(Value::Null, |(_, v)| v.clone());
                        (col.clone(), val.as_bytes())
                    })
                    .collect();
                let entry = groups.entry(key).or_default();
                for (name, func) in &aggs {
                    match func {
                        AggFunc::CountStar => {
                            *entry.entry(name.clone()).or_insert(0) += 1;
                        }
                        AggFunc::CountCol(col) => {
                            let non_null = row
                                .iter()
                                .find(|(n, _)| n == col)
                                .is_some_and(|(_, v)| !matches!(v, Value::Null));
                            if non_null {
                                *entry.entry(name.clone()).or_insert(0) += 1;
                            }
                        }
                    }
                }
            }
            #[allow(clippy::needless_collect, clippy::type_complexity)]
            let out: Vec<(Vec<(String, Vec<u8>)>, Vec<(String, Value)>)> = groups
                .into_iter()
                .map(|(gkey, counts)| {
                    let mut fields = gkey
                        .into_iter()
                        .map(|(n, bytes)| (n, Value::Bytes(bytes)))
                        .collect::<Vec<_>>();
                    for (name, count) in counts {
                        fields.push((
                            name,
                            Value::Integer(i64::try_from(count).unwrap_or(i64::MAX)),
                        ));
                    }
                    (Vec::new(), fields)
                })
                .collect();
            Ok(ExecState::Aggregate {
                groups: out.into_iter(),
            })
        }
        PhysicalPlan::Sort { input, keys } => {
            let mut rows = execute_to_rows(*input, ctx)?;
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
    }
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

fn merge_rows(left: &RowMap, right: &RowMap) -> RowMap {
    let mut out = left.clone();
    out.extend(right.clone());
    out
}

fn compare_rows(a: &RowMap, b: &RowMap, keys: &[(String, bool)]) -> std::cmp::Ordering {
    for (col, asc) in keys {
        let va = a.iter().find(|(n, _)| n == col).map(|(_, v)| v.as_bytes());
        let vb = b.iter().find(|(n, _)| n == col).map(|(_, v)| v.as_bytes());
        let ord = va.cmp(&vb);
        if ord != std::cmp::Ordering::Equal {
            return if *asc { ord } else { ord.reverse() };
        }
    }
    std::cmp::Ordering::Equal
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

    let mut row = RowMap::new();
    for (key, val) in StorageEngine::iter(store) {
        if !key.starts_with(&prefix) {
            if key.as_slice() > prefix.as_slice() {
                break;
            }
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
                columns: None,
            }),
            right: Box::new(PhysicalPlan::SeqScan {
                table: "orders".into(),
                columns: None,
            }),
            on: on_expr,
            left_key: keys.left,
            right_key: keys.right,
        };
        let rows = execute(plan, &ctx).unwrap();
        assert_eq!(rows.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
