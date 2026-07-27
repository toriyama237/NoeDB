//! Query planner and executor for NoeDB (Phase 3).
//!
//! Weeks 17–28: logical/physical plans, cost-based optimizer, B-tree indexes,
//! Volcano executors (`SeqScan`, `IndexScan`, `HashJoin`, …), and `EXPLAIN`.

#![forbid(unsafe_code)]
#![allow(unreachable_pub)] // API surface re-exported by the `noedb` meta-crate.

mod adaptive;
mod aggregate;
mod build;
mod cast;
mod cost;
mod cte;
mod ddl;
mod eval;
mod executor;
mod explain;
mod fold;
mod index;
mod join;
mod logical;
mod lower;
mod optimize;
mod parallel;
mod physical;
mod pk;
mod schema;
mod setops;
mod simd_pred;
mod star;
mod stats;
mod stream_agg;
mod subquery;
mod value;
mod window;
mod window_exec;

pub use adaptive::ExecutionFeedback;
pub use build::{build, build_select, build_with_schema, cte_names_from};
pub use cost::{
    estimate, index_beats_seq_scan, PlanStats, INDEX_LOOKUP_COST, SEQ_SCAN_ROW_COST,
    UNKNOWN_TABLE_ROWS,
};
pub use ddl::purge_table_data;
pub use eval::{eval_expr, eval_predicate};
pub use executor::{execute, ExecutionContext, Executor};
pub use explain::explain;
pub use index::{BTreeIndex, SecondaryIndex};
pub use logical::{AggFunc, LogicalPlan};
pub use lower::lower;
pub use optimize::{index_wins, optimize, PlanContext};
pub use physical::PhysicalPlan;
pub use pk::{mark_row_id_column, row_id_column, unmark_row_id_column};
pub use schema::QuerySchema;
pub use simd_pred::{filter_eq_i64, filter_range_i64};
pub use stats::{
    analyze_table, increment_row_count, load_plan_stats, persist_table_stats, ColumnStats,
    TableStats,
};
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
    execute_sql_with_schema(stmt, store, store, None)
}

/// Execute with optional schema catalog (`SELECT *` expansion).
///
/// # Errors
///
/// Planner or executor errors.
pub fn execute_sql_with_schema<S: StorageEngine<Error = StorageError>>(
    stmt: &Statement,
    exec_store: &S,
    index_store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<Vec<Record>, ExecError> {
    execute_sql_on_with_schema(stmt, exec_store, index_store, schema)
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
    execute_sql_on_with_schema(stmt, exec_store, index_store, None)
}

/// Execute with separate stores and optional schema catalog.
///
/// # Errors
///
/// Planner or executor errors.
pub fn execute_sql_on_with_schema<S: StorageEngine<Error = StorageError>>(
    stmt: &Statement,
    exec_store: &S,
    index_store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<Vec<Record>, ExecError> {
    let folded = fold::fold_scalar_subqueries(stmt, exec_store, index_store, schema)?;
    let stmt = folded.as_ref().unwrap_or(stmt);
    let cte_tables = if let Statement::Select(s) = stmt {
        s.with_clause
            .as_ref()
            .map(|with| cte::materialize_with_clause(with, exec_store, index_store, schema))
            .transpose()?
            .unwrap_or_default()
    } else {
        HashMap::new()
    };

    let logical = build_with_schema(stmt, schema)?;
    let stats = load_plan_stats(index_store);
    let ctx = PlanContext::with_stats(index_store, stats);
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
    explain_sql_with_schema(stmt, store, None)
}

/// Return an `EXPLAIN` plan with optional schema catalog.
///
/// # Errors
///
/// Planner errors for unsupported statements.
pub fn explain_sql_with_schema(
    stmt: &Statement,
    store: &LsmTree,
    schema: Option<&QuerySchema>,
) -> Result<String, PlanError> {
    let folded = fold::fold_scalar_subqueries(stmt, store, store, schema)
        .ok()
        .flatten();
    let stmt = folded.as_ref().unwrap_or(stmt);
    let logical = build_with_schema(stmt, schema)?;
    let stats = load_plan_stats(store);
    let ctx = PlanContext::with_stats(store, stats);
    let physical = optimize(logical, &ctx);
    Ok(explain(&physical, &ctx.stats))
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
            let table_stats = analyze_table(store, &idx.table.value);
            persist_table_stats(store, &idx.table.value, &table_stats)?;
            Ok(vec![])
        }
        Statement::AnalyzeTable(a) => {
            let table_stats = analyze_table(store, &a.table.value);
            persist_table_stats(store, &a.table.value, &table_stats)?;
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
        stats.tables.entry(table.clone()).or_default().row_count = *rows;
    }
}

/// An error produced by the planner.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanError {
    /// Statement kind not implemented yet.
    UnsupportedStatement,
    /// Table not found in schema catalog.
    UnknownTable {
        /// Table name.
        name: String,
    },
    /// `SELECT *` requires a schema catalog.
    MissingSchema,
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

    fn put_packed(tree: &mut LsmTree, table: &str, row: &str, cols: &[(&str, &[u8])]) {
        let key = noedb_storage::packed_row_key(table, row);
        let record = noedb_storage::encode_row(
            &cols
                .iter()
                .map(|(n, v)| ((*n).to_string(), v.to_vec()))
                .collect::<Vec<_>>(),
        );
        tree.put(&key, &record).unwrap();
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

    #[test]
    fn select_star_expanded_with_schema() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"1");
        put_row(&mut tree, "users", "1", "name", b"ada");
        let schema = QuerySchema::from_tables([(
            "users".to_string(),
            vec!["id".to_string(), "name".to_string()],
        )]);
        let stmt = noedb_parser::parse("SELECT * FROM users").unwrap();
        let rows = execute_sql_with_schema(&stmt, &tree, &tree, Some(&schema)).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn order_by_limit() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "name", b"bob");
        put_row(&mut tree, "users", "2", "name", b"ada");
        let stmt = noedb_parser::parse("SELECT name FROM users ORDER BY name LIMIT 1").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"ada".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn count_aggregate() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"1");
        put_row(&mut tree, "users", "2", "id", b"2");
        put_row(&mut tree, "users", "3", "id", b"3");
        let stmt = noedb_parser::parse("SELECT COUNT(id) FROM users").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Integer(3));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn like_filter() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "name", b"Marie");
        put_row(&mut tree, "users", "2", "name", b"Paul");
        let stmt = noedb_parser::parse("SELECT name FROM users WHERE name LIKE 'M%'").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"Marie".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn scalar_upper() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "name", b"ada");
        let stmt = noedb_parser::parse("SELECT UPPER(name) FROM users").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"ADA".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sum_aggregate() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "orders", "1", "amount", b"10");
        put_row(&mut tree, "orders", "2", "amount", b"25");
        let stmt = noedb_parser::parse("SELECT SUM(amount) FROM orders").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Float(35.0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn count_star_aggregate() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"1");
        put_row(&mut tree, "users", "2", "id", b"2");
        let stmt = noedb_parser::parse("SELECT COUNT(*) FROM users").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Integer(2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn in_list_filter() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"52");
        put_row(&mut tree, "users", "2", "id", b"99");
        put_row(&mut tree, "users", "3", "id", b"53");
        let stmt = noedb_parser::parse("SELECT id FROM users WHERE id IN ('52', '53')").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn select_distinct() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "name", b"ada");
        put_row(&mut tree, "users", "2", "name", b"ada");
        put_row(&mut tree, "users", "3", "name", b"bob");
        let stmt = noedb_parser::parse("SELECT DISTINCT name FROM users").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn between_filter() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "id", b"3");
        put_row(&mut tree, "users", "2", "id", b"7");
        put_row(&mut tree, "users", "3", "id", b"9");
        let stmt =
            noedb_parser::parse("SELECT id FROM users WHERE id BETWEEN '5' AND '8'").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"7".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn group_by_count() {
        let (mut tree, dir) = temp_tree();
        put_row(&mut tree, "users", "1", "campus", b"A");
        put_row(&mut tree, "users", "1", "id", b"1");
        put_row(&mut tree, "users", "2", "campus", b"A");
        put_row(&mut tree, "users", "2", "id", b"2");
        put_row(&mut tree, "users", "3", "campus", b"B");
        put_row(&mut tree, "users", "3", "id", b"3");
        let stmt =
            noedb_parser::parse("SELECT campus, COUNT(id) FROM users GROUP BY campus").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn optimize_fuses_limit_into_sort_top_k() {
        let (mut tree, dir) = temp_tree();
        for i in 1..=20 {
            put_row(
                &mut tree,
                "t",
                &i.to_string(),
                "v",
                i.to_string().as_bytes(),
            );
        }
        let stmt = noedb_parser::parse("SELECT v FROM t ORDER BY v DESC LIMIT 5").unwrap();
        let text = explain_sql(&stmt, &tree).unwrap();
        assert!(
            text.contains("top_k=5"),
            "expected fused top-k in plan: {text}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    // v2.4: streaming aggregates + primary-key point lookup.

    #[test]
    fn streaming_count_star_matches_row_count_on_packed_rows() {
        let (mut tree, dir) = temp_tree();
        for i in 1..=37 {
            put_packed(
                &mut tree,
                "emp",
                &i.to_string(),
                &[("id", i.to_string().as_bytes())],
            );
        }
        let stmt = noedb_parser::parse("SELECT COUNT(*) FROM emp").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows[0].fields[0].1, Value::Integer(37));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn streaming_aggregate_with_predicate_and_group_by_on_packed_rows() {
        let (mut tree, dir) = temp_tree();
        let data = [("A", 10), ("A", 20), ("B", 5), ("B", 100), ("A", 1)];
        for (i, (dept, salary)) in data.iter().enumerate() {
            put_packed(
                &mut tree,
                "emp",
                &i.to_string(),
                &[
                    ("dept", dept.as_bytes()),
                    ("salary", salary.to_string().as_bytes()),
                ],
            );
        }
        let stmt = noedb_parser::parse(
            "SELECT dept, SUM(salary) AS s FROM emp WHERE salary > 5 GROUP BY dept",
        )
        .unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        let mut got: Vec<(Vec<u8>, f64)> = rows
            .iter()
            .map(|r| {
                let Value::Bytes(dept) = &r.fields[0].1 else {
                    panic!("expected bytes")
                };
                let Value::Float(s) = &r.fields[1].1 else {
                    panic!("expected float")
                };
                (dept.clone(), *s)
            })
            .collect();
        got.sort_by(|a, b| a.0.cmp(&b.0));
        // A: 10 + 20 = 30 (1 excluded by WHERE salary > 5); B: 100 (5 excluded).
        assert_eq!(got, vec![(b"A".to_vec(), 30.0), (b"B".to_vec(), 100.0)]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn streaming_aggregate_matches_generic_path_with_cell_override() {
        // Packed row plus a legacy cell for the same column: the cell must
        // win, exactly like the row-materializing scan path.
        let (mut tree, dir) = temp_tree();
        put_packed(&mut tree, "emp", "1", &[("salary", b"10")]);
        put_row(&mut tree, "emp", "1", "salary", b"999");
        let stmt = noedb_parser::parse("SELECT SUM(salary) FROM emp").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows[0].fields[0].1, Value::Float(999.0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn streaming_aggregate_empty_table_yields_one_zero_row() {
        let (tree, dir) = temp_tree();
        let stmt = noedb_parser::parse("SELECT COUNT(*) FROM emp").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Integer(0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn streaming_aggregate_falls_back_for_non_streamable_predicate() {
        // `IN (SELECT …)` cannot be evaluated by the streaming pass; the
        // generic path must still produce the right answer.
        let (mut tree, dir) = temp_tree();
        put_packed(&mut tree, "emp", "1", &[("dept", b"A")]);
        put_packed(&mut tree, "emp", "2", &[("dept", b"B")]);
        put_packed(&mut tree, "ok_depts", "1", &[("name", b"A")]);
        let stmt = noedb_parser::parse(
            "SELECT COUNT(*) FROM emp WHERE dept IN (SELECT name FROM ok_depts)",
        )
        .unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows[0].fields[0].1, Value::Integer(1));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pk_lookup_used_for_equality_on_marked_column() {
        let (mut tree, dir) = temp_tree();
        put_packed(&mut tree, "emp", "42", &[("id", b"42"), ("name", b"Ada")]);
        mark_row_id_column(&mut tree, "emp", "id").unwrap();

        let stmt = noedb_parser::parse("SELECT name FROM emp WHERE id = 42").unwrap();
        let text = explain_sql(&stmt, &tree).unwrap();
        assert!(
            text.contains("PkLookup"),
            "expected PkLookup in plan: {text}"
        );

        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"Ada".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pk_lookup_miss_returns_no_rows() {
        let (mut tree, dir) = temp_tree();
        put_packed(&mut tree, "emp", "42", &[("id", b"42"), ("name", b"Ada")]);
        mark_row_id_column(&mut tree, "emp", "id").unwrap();

        let stmt = noedb_parser::parse("SELECT name FROM emp WHERE id = 7").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert!(rows.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn equality_on_unmarked_column_does_not_use_pk_lookup() {
        // No marker persisted for `emp`: even though `id` happens to be a
        // plausible primary key name, the planner must not assume it is
        // the row-id column and must fall back to a full scan/filter.
        let (mut tree, dir) = temp_tree();
        put_packed(&mut tree, "emp", "1", &[("id", b"99"), ("name", b"Ada")]);
        let stmt = noedb_parser::parse("SELECT name FROM emp WHERE id = 99").unwrap();
        let text = explain_sql(&stmt, &tree).unwrap();
        assert!(
            !text.contains("PkLookup"),
            "unexpected PkLookup in plan: {text}"
        );
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"Ada".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn equality_on_non_pk_column_never_uses_pk_lookup_even_if_value_matches_a_row_id() {
        // `emp` row-id is `id`; a row happens to exist whose id equals the
        // *value* being searched for on `salary`. If the planner ever
        // conflated "any equality on the row-id-shaped value" with
        // "equality on the pk column", this would incorrectly return the
        // wrong row.
        let (mut tree, dir) = temp_tree();
        mark_row_id_column(&mut tree, "emp", "id").unwrap();
        put_packed(&mut tree, "emp", "1", &[("id", b"1"), ("salary", b"5000")]);
        put_packed(
            &mut tree,
            "emp",
            "5000",
            &[("id", b"5000"), ("salary", b"1")],
        );
        let stmt = noedb_parser::parse("SELECT id FROM emp WHERE salary = 5000").unwrap();
        let rows = execute_sql(&stmt, &tree).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields[0].1, Value::Bytes(b"1".to_vec()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
