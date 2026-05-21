//! Volcano-style executor (Weeks 18–20).

use std::collections::BTreeMap;

use noedb_ast::{Expr, SelectItem};
use noedb_storage::{LsmTree, StorageEngine};

use crate::eval::{eval_expr, eval_predicate};
use crate::physical::PhysicalPlan;
use crate::value::{Record, Value};
use crate::ExecError;

/// Storage-backed execution context.
pub struct ExecutionContext<'a> {
    /// LSM engine to read from.
    pub store: &'a LsmTree,
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
    Filter {
        predicate: Expr,
        child: Box<Executor>,
    },
    Project {
        items: Vec<SelectItem>,
        child: Box<Executor>,
    },
}

type RowMap = Vec<(String, Value)>;

impl Executor {
    /// Build an executor for `plan`.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(plan: PhysicalPlan, ctx: &ExecutionContext<'_>) -> Result<Self, ExecError> {
        let state = build_state(plan, ctx)?;
        Ok(Self { state })
    }

    /// Pull the next result row.
    #[allow(clippy::never_loop)] // state machine: at most one transition per call
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
                    return Ok(Some(Record {
                        fields: cells
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| (format!("col{i}"), v))
                            .collect(),
                    }));
                }
                ExecState::SeqScan { rows } => {
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
                        let fields = items
                            .iter()
                            .zip(cells)
                            .map(|(item, v)| {
                                let name = item.alias.as_ref().map_or_else(
                                    || "col".into(),
                                    |a| a.value.clone(),
                                );
                                (name, v)
                            })
                            .collect();
                        return Ok(Some(Record { fields }));
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
fn build_state(plan: PhysicalPlan, ctx: &ExecutionContext<'_>) -> Result<ExecState, ExecError> {
    match plan {
        PhysicalPlan::SeqScan { table } if table.is_empty() => Ok(ExecState::LiteralProject {
            items: vec![],
            emitted: false,
        }),
        PhysicalPlan::SeqScan { table } => {
            let rows = load_table_rows(ctx.store, &table);
            Ok(ExecState::SeqScan {
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
            if matches!(&*input, PhysicalPlan::SeqScan { table } if table.is_empty()) {
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
        PhysicalPlan::HashJoin { .. } => Err(ExecError::UnsupportedOperator),
    }
}

/// Row keys: `table\0row_id\0column` → cell value; collapsed to one row per `row_id`.
#[allow(clippy::option_if_let_else)]
fn load_table_rows(store: &LsmTree, table: &str) -> Vec<RowMap> {
    let mut prefix = table.as_bytes().to_vec();
    prefix.push(0);

    let mut grouped: BTreeMap<Vec<u8>, RowMap> = BTreeMap::new();
    for (key, val) in StorageEngine::iter(store) {
        if !key.starts_with(&prefix) {
            continue;
        }
        let rest = &key[prefix.len()..];
        if let Some(pos) = rest.iter().position(|&b| b == 0) {
            let (row_id, col) = rest.split_at(pos);
            let col = &col[1..];
            let col_name = String::from_utf8_lossy(col).into_owned();
            grouped
                .entry(row_id.to_vec())
                .or_default()
                .push((col_name, Value::Bytes(val)));
        } else {
            grouped
                .entry(rest.to_vec())
                .or_default()
                .push(("value".into(), Value::Bytes(val)));
        }
    }

    grouped.into_values().collect()
}

/// Execute a physical plan and return all rows.
pub fn execute(plan: PhysicalPlan, ctx: &ExecutionContext<'_>) -> Result<Vec<Record>, ExecError> {
    Executor::new(plan, ctx)?.collect()
}
