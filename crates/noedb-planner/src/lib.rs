//! Query planner and executor for NoeDB (Phase 3).
//!
//! Weeks 17–28: logical/physical plans, cost-based optimizer, B-tree indexes,
//! Volcano executors (`SeqScan`, `IndexScan`, `HashJoin`, …), and `EXPLAIN`.

#![forbid(unsafe_code)]
#![allow(unreachable_pub)] // API surface re-exported by the `noedb` meta-crate.

mod adaptive;
mod build;
mod cost;
mod eval;
mod executor;
mod explain;
mod index;
mod join;
mod logical;
mod lower;
mod optimize;
mod parallel;
mod physical;
mod simd_pred;
mod value;
mod cte;
mod subquery;
mod window;
mod window_exec;

pub use adaptive::ExecutionFeedback;
pub use build::build;
pub use cost::{estimate, PlanStats, INDEX_LOOKUP_COST, SEQ_SCAN_ROW_COST};
pub use executor::{execute, ExecutionContext, Executor};
pub use explain::explain;
pub use index::{BTreeIndex, SecondaryIndex};
pub use logical::{AggFunc, LogicalPlan};
pub use lower::lower;
pub use optimize::{index_wins, optimize, PlanContext};
pub use physical::PhysicalPlan;
pub use simd_pred::{filter_eq_i64, filter_range_i64};
pub use value::{Record, Value};

use noedb_ast::Statement;
use noedb_storage::{LsmTree, StorageEngine, StorageError};

use std::collections::HashMap;

/// Turn a [`Statement`] into a [`LogicalPlan`].
///
/// # Errors
///
/// Returns [`PlanError::UnsupportedStatement`] for non-`SELECT` statements.
pub fn plan(stmt: &Statement) -> Result<LogicalPlan, PlanError> {
    build(stmt)
}

/// Plan, optimize, and execute a `SELECT` against storage.
///
/// # Errors
///
/// Planner or executor errors.
pub fn execute_sql(stmt: &Statement, store: &LsmTree) -> Result<Vec<Record>, ExecError> {
    execute_sql_on(stmt, store, store)
}

/// Execute with separate read store (MVCC snapshot) and index catalog store.
///
/// # Errors
///
/// Planner or executor errors.
pub fn execute_sql_on<S: StorageEngine<Error = StorageError>>(
    stmt: &Statement,
    exec_store: &S,
    index_store: &LsmTree,
) -> Result<Vec<Record>, ExecError> {
    let cte_tables = if let Statement::Select(s) = stmt {
        s.with_clause
            .as_ref()
            .map(|with| cte::materialize_with_clause(with, exec_store, index_store))
            .transpose()?
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let logical = plan(stmt)?;
    let ctx = PlanContext::new(index_store);
    let physical = optimize(logical, &ctx);
    execute(
        physical,
        &ExecutionContext {
            store: exec_store,
            index_catalog: index_store,
            cte_tables: if cte_tables.is_empty() {
                None
            } else {
                Some(&cte_tables)
            },
        },
    )
}

/// Return an `EXPLAIN` plan for a `SELECT` (optimized physical plan + cost).
///
/// # Errors
///
/// Planner errors for unsupported statements.
pub fn explain_sql(stmt: &Statement, store: &LsmTree) -> Result<String, PlanError> {
    let logical = plan(stmt)?;
    let ctx = PlanContext::new(store);
    let physical = optimize(logical, &ctx);
    Ok(explain(&physical))
}

/// Apply DDL (`CREATE INDEX`) or run DML/query statements.
///
/// # Errors
///
/// Planner, executor, or storage errors.
pub fn apply_statement(stmt: &Statement, store: &mut LsmTree) -> Result<Vec<Record>, ExecError> {
    match stmt {
        Statement::CreateIndex(idx) => {
            if idx.columns.len() != 1 {
                return Err(ExecError::UnsupportedExpr);
            }
            SecondaryIndex::build(store, &idx.table.value, &idx.columns[0].value)?;
            Ok(vec![])
        }
        _ => execute_sql(stmt, store),
    }
}

/// Build a secondary index on `(table, column)`.
///
/// # Errors
///
/// Storage write failures.
pub fn create_index(store: &mut LsmTree, table: &str, column: &str) -> Result<(), StorageError> {
    SecondaryIndex::build(store, table, column)
}

/// Merge runtime feedback into planner statistics (adaptive costing).
pub fn record_execution(stats: &mut PlanStats, feedback: &ExecutionFeedback) {
    for (table, rows) in &feedback.table_rows {
        stats.table_rows.insert(table.clone(), *rows);
    }
}

/// An error produced by the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanError {
    /// Statement kind not implemented yet.
    UnsupportedStatement,
}

/// An error produced at execution time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecError {
    /// Expression form not supported yet.
    UnsupportedExpr,
    /// Physical operator not implemented yet.
    UnsupportedOperator,
    /// Column not found in row.
    UnknownColumn {
        /// Column name.
        name: String,
    },
    /// Type mismatch during evaluation.
    TypeMismatch {
        /// Human-readable detail.
        message: String,
    },
    /// Planner failure.
    Plan(PlanError),
    /// Storage layer failure.
    Storage(StorageError),
}

impl From<PlanError> for ExecError {
    fn from(e: PlanError) -> Self {
        Self::Plan(e)
    }
}

impl From<StorageError> for ExecError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use noedb_storage::{LsmConfig, LsmTree};

    fn temp_tree() -> (LsmTree, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "noedb-planner-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        (tree, dir)
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
    fn plan_select_one_literal() {
        let stmt = noedb_parser::parse("SELECT 1").unwrap();
        let logical = plan(&stmt).unwrap();
        let physical = lower(logical);
        assert!(matches!(physical, PhysicalPlan::Project { .. }));
    }

    #[test]
    fn execute_select_literal() {
        let stmt = noedb_parser::parse("SELECT 1").unwrap();
        let (tree, dir) = temp_tree();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Integer(1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_select_from_table() {
        let stmt = noedb_parser::parse("SELECT name FROM users").unwrap();
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "name", b"ada");
        put_row(&mut tree, "users", "2", "name", b"bob");
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_cte_join_on_parent_id() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "nodes", "a", "id", b"1");
        put_row(&mut tree, "nodes", "a", "parent_id", b"0");
        put_row(&mut tree, "nodes", "b", "id", b"2");
        put_row(&mut tree, "nodes", "b", "parent_id", b"1");
        let stmt = noedb_parser::parse(
            "WITH tree AS (SELECT id FROM nodes WHERE id = '1') \
             SELECT n.id FROM nodes n INNER JOIN tree t ON n.parent_id = t.id",
        )
        .unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"2".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn explain_uses_index_scan() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"7");
        put_row(&mut tree, "users", "1", "name", b"ada");
        create_index(&mut tree, "users", "id").unwrap();
        let stmt = noedb_parser::parse("SELECT name FROM users WHERE id = '7'").unwrap();
        let text = explain_sql(&stmt, &tree).unwrap();
        assert!(text.contains("IndexScan"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn execute_join() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"1");
        put_row(&mut tree, "users", "1", "name", b"ada");
        put_row(&mut tree, "orders", "9", "user_id", b"1");
        put_row(&mut tree, "orders", "9", "sku", b"book");
        let stmt = noedb_parser::parse(
            "SELECT name FROM users INNER JOIN orders ON users.id = orders.user_id",
        )
        .unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn create_index_via_ddl() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"1");
        let stmt = noedb_parser::parse("CREATE INDEX idx_users_id ON users (id)").unwrap();
        apply_statement(&stmt, &mut tree).unwrap();
        assert!(SecondaryIndex::exists(&tree, "users", "id"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
