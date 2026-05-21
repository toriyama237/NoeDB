//! Query planner and executor for NoeDB (Phase 3).
//!
//! Week 17–20: logical/physical plans, naive lowering, `SeqScan` / `Filter` /
//! `Project` executors over [`LsmTree`].

#![forbid(unsafe_code)]
#![allow(unreachable_pub)] // API surface re-exported by the `noedb` meta-crate.

mod build;
mod eval;
mod executor;
mod lower;
mod logical;
mod physical;
mod value;

pub use build::build;
pub use executor::{execute, ExecutionContext, Executor};
pub use logical::LogicalPlan;
pub use lower::lower;
pub use physical::PhysicalPlan;
pub use value::{Record, Value};

use noedb_ast::Statement;

/// Turn a [`Statement`] into a [`LogicalPlan`].
///
/// # Errors
///
/// Returns [`PlanError::UnsupportedStatement`] for non-`SELECT` statements.
pub fn plan(stmt: &Statement) -> Result<LogicalPlan, PlanError> {
    build(stmt)
}

/// Plan, lower, and execute a `SELECT` against storage.
///
/// # Errors
///
/// Planner or executor errors.
pub fn execute_sql(stmt: &Statement, store: &noedb_storage::LsmTree) -> Result<Vec<Record>, ExecError> {
    let logical = plan(stmt)?;
    let physical = lower(logical);
    let ctx = ExecutionContext { store };
    execute(physical, &ctx)
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
    Storage(noedb_storage::StorageError),
}

impl From<PlanError> for ExecError {
    fn from(e: PlanError) -> Self {
        Self::Plan(e)
    }
}

impl From<noedb_storage::StorageError> for ExecError {
    fn from(e: noedb_storage::StorageError) -> Self {
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
}
