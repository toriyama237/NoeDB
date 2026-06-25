//! CTE materialization (Phase 5 Week 40).

use std::collections::{HashMap, HashSet};

use noedb_ast::{CteBody, SelectStmt, WithClause};
use noedb_storage::{LsmTree, StorageEngine, StorageError};

use crate::build::{build_select_scoped, cte_names_from};
use crate::executor::{execute, ExecutionContext, RowMap};
use crate::optimize::{optimize, PlanContext};
use crate::schema::QuerySchema;
use crate::ExecError;

/// Materialize all CTEs in a `WITH` clause (in definition order).
pub fn materialize_with_clause<S: StorageEngine<Error = StorageError>>(
    with: &WithClause,
    exec_store: &S,
    index_store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<HashMap<String, Vec<RowMap>>, ExecError> {
    let mut tables: HashMap<String, Vec<RowMap>> = HashMap::new();
    let mut scope: HashSet<String> = HashSet::new();

    for cte in &with.ctes {
        let rows = match &cte.body {
            CteBody::Select(stmt) => {
                run_select(stmt, &scope, &tables, exec_store, index_store, schema)?
            }
            CteBody::Union {
                anchor,
                all: _,
                recursive,
            } => {
                if !with.recursive {
                    return Err(ExecError::UnsupportedExpr);
                }
                materialize_recursive(
                    &cte.name.value,
                    anchor,
                    recursive,
                    &scope,
                    &tables,
                    exec_store,
                    index_store,
                    schema,
                )?
            }
        };
        tables.insert(cte.name.value.clone(), rows);
        scope.insert(cte.name.value.clone());
    }
    Ok(tables)
}

#[allow(clippy::too_many_arguments)]
fn materialize_recursive<S: StorageEngine<Error = StorageError>>(
    name: &str,
    anchor: &SelectStmt,
    recursive: &SelectStmt,
    prior_scope: &HashSet<String>,
    prior_tables: &HashMap<String, Vec<RowMap>>,
    exec_store: &S,
    index_store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<Vec<RowMap>, ExecError> {
    let mut acc = run_select(
        anchor,
        prior_scope,
        prior_tables,
        exec_store,
        index_store,
        schema,
    )?;
    let mut working = prior_tables.clone();
    working.insert(name.to_string(), acc.clone());

    let mut scope = prior_scope.clone();
    scope.insert(name.to_string());

    loop {
        let new_rows = run_select(recursive, &scope, &working, exec_store, index_store, schema)?;
        let mut seen: HashSet<Vec<u8>> = acc.iter().map(crate::setops::row_key).collect();
        let mut added = 0usize;
        for row in new_rows {
            let key = crate::setops::row_key(&row);
            if seen.insert(key) {
                acc.push(row);
                added += 1;
            }
        }
        if added == 0 {
            break;
        }
        working.insert(name.to_string(), acc.clone());
    }
    Ok(acc)
}

fn run_select<S: StorageEngine<Error = StorageError>>(
    stmt: &SelectStmt,
    cte_scope: &HashSet<String>,
    cte_tables: &HashMap<String, Vec<RowMap>>,
    exec_store: &S,
    index_store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<Vec<RowMap>, ExecError> {
    let mut scope = cte_scope.clone();
    scope.extend(cte_names_from(stmt.with_clause.as_ref()));
    let logical = build_select_scoped(stmt, &scope, schema)?;
    let plan_ctx = PlanContext::with_stats(index_store, crate::load_plan_stats(index_store));
    let physical = optimize(logical, &plan_ctx);
    let records = execute(
        physical,
        &ExecutionContext {
            store: exec_store,
            index_catalog: index_store,
            cte_tables: Some(cte_tables),
        },
    )?;
    Ok(records.into_iter().map(|r| r.fields).collect())
}
