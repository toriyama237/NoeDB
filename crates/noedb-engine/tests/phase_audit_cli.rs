//! Audit-driven CLI regressions (Noe Engineering functional audit v2.0.0).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-audit-cli-{}-{}-{}",
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn not_operator_negates_bool() {
    let (eng, dir) = temp_engine();
    let out = eng.execute("SELECT NOT true").unwrap();
    assert_eq!(out.rows[0][0], "false");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn arithmetic_mul_div_mod() {
    let (eng, dir) = temp_engine();
    assert_eq!(eng.execute("SELECT 4 * 5").unwrap().rows[0][0], "20");
    assert_eq!(eng.execute("SELECT 1 / 2").unwrap().rows[0][0], "0.5");
    assert_eq!(eng.execute("SELECT 10 % 3").unwrap().rows[0][0], "1");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bool_display_and_filter() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT, name TEXT, active BOOL)")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u1', 'Alice', true)")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u2', 'Bob', false)")
        .unwrap();

    let display = eng.execute("SELECT id, active FROM users").unwrap();
    assert_eq!(display.rows[0][1], "true");
    assert_eq!(display.rows[1][1], "false");

    let filtered = eng
        .execute("SELECT id FROM users WHERE active = true")
        .unwrap();
    assert_eq!(filtered.rows.len(), 1);
    assert_eq!(filtered.rows[0][0], "u1");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn is_null_predicates() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs (id TEXT, content TEXT)")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES ('d1', 'hello')")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES ('d4', NULL)").unwrap();

    let nulls = eng
        .execute("SELECT id FROM docs WHERE content IS NULL")
        .unwrap();
    assert_eq!(nulls.rows, vec![vec!["d4".to_string()]]);

    let non_nulls = eng
        .execute("SELECT id FROM docs WHERE content IS NOT NULL")
        .unwrap();
    assert_eq!(non_nulls.rows, vec![vec!["d1".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn create_table_rejects_duplicate() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT, name TEXT)")
        .unwrap();
    let err = eng.execute("CREATE TABLE users (id TEXT)").unwrap_err();
    assert!(err.to_string().contains("already exists"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn create_table_if_not_exists_is_idempotent() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT)").unwrap();
    eng.execute("CREATE TABLE IF NOT EXISTS users (id TEXT)")
        .unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn drop_table_removes_catalog_entry() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs2 (id TEXT)").unwrap();
    eng.execute("DROP TABLE docs2").unwrap();
    let err = eng.execute("SELECT * FROM docs2").unwrap_err();
    assert!(err.to_string().contains("docs2"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn schema_persists_across_reopen() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-audit-persist-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let eng = LocalEngine::open(&dir).unwrap();
        eng.execute("CREATE TABLE accounts (id TEXT, balance INT)")
            .unwrap();
        eng.execute("INSERT INTO accounts VALUES ('a1', 100)")
            .unwrap();
    }
    let eng = LocalEngine::open(&dir).unwrap();
    let out = eng.execute("SELECT id, balance FROM accounts").unwrap();
    assert_eq!(out.rows, vec![vec!["a1".to_string(), "100".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cte_resolves_schema() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE orders (id TEXT, status TEXT)")
        .unwrap();
    eng.execute("INSERT INTO orders VALUES ('o1', 'paid')")
        .unwrap();
    let out = eng
        .execute(
            "WITH paid_orders AS (SELECT id FROM orders WHERE status = 'paid') \
             SELECT id FROM paid_orders",
        )
        .unwrap();
    assert_eq!(out.rows, vec![vec!["o1".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn union_dedupes_across_branches() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT, name TEXT)")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u1', 'Alice')")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u2', 'Alice')")
        .unwrap();
    eng.execute("CREATE TABLE docs (id TEXT, content TEXT)")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES ('d1', 'Alice')")
        .unwrap();

    let out = eng
        .execute("SELECT name FROM users UNION SELECT content FROM docs")
        .unwrap();
    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.rows[0][0], "Alice");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn scalar_subquery_in_where() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT, age INT)")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u1', 30)").unwrap();
    eng.execute("INSERT INTO users VALUES ('u2', 20)").unwrap();
    eng.execute("INSERT INTO users VALUES ('u3', 40)").unwrap();

    let out = eng
        .execute("SELECT id FROM users WHERE age > (SELECT AVG(age) FROM users)")
        .unwrap();
    assert_eq!(out.rows, vec![vec!["u3".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn alter_table_add_column() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT, name TEXT)")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u1', 'Alice')")
        .unwrap();
    eng.execute("ALTER TABLE users ADD COLUMN email TEXT")
        .unwrap();
    eng.execute("INSERT INTO users VALUES ('u2', 'Bob', 'bob@x.com')")
        .unwrap();
    let out = eng
        .execute("SELECT id, email FROM users WHERE id = 'u2'")
        .unwrap();
    assert_eq!(
        out.rows,
        vec![vec!["u2".to_string(), "bob@x.com".to_string()]]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn alter_table_rejects_duplicate_column() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE users (id TEXT, name TEXT)")
        .unwrap();
    let err = eng
        .execute("ALTER TABLE users ADD COLUMN name TEXT")
        .unwrap_err();
    assert!(err.to_string().contains("column already exists"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn primary_key_rejects_duplicate_insert() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs (id TEXT PRIMARY KEY, content TEXT)")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES ('d1', 'first')")
        .unwrap();
    let err = eng
        .execute("INSERT INTO docs VALUES ('d1', 'dup')")
        .unwrap_err();
    assert!(err.to_string().contains("PRIMARY KEY"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unique_constraint_rejects_duplicate() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs (id TEXT PRIMARY KEY, email TEXT UNIQUE)")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES ('d1', 'a@x.com')")
        .unwrap();
    let err = eng
        .execute("INSERT INTO docs VALUES ('d2', 'a@x.com')")
        .unwrap_err();
    assert!(err.to_string().contains("UNIQUE"));
    eng.execute("INSERT INTO docs VALUES ('d2', 'b@x.com')")
        .unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explain_hides_internal_sort_column() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs (id TEXT, embedding VECTOR(3))")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES ('d1', '[1.0, 2.0, 3.0]')")
        .unwrap();
    let plan = eng
        .explain("SELECT id FROM docs ORDER BY embedding <-> '[1.0,1.0,1.0]' LIMIT 1")
        .unwrap();
    assert!(!plan.contains("__sort"), "internal column leaked: {plan}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn not_null_rejected_insert_leaves_no_ghost_row() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE t (id INT PRIMARY KEY, name TEXT NOT NULL)")
        .unwrap();
    eng.execute("INSERT INTO t VALUES (1, 'Alice')").unwrap();
    let err = eng.execute("INSERT INTO t VALUES (6, NULL)").unwrap_err();
    assert!(err.to_string().contains("NOT NULL"));
    let out = eng.execute("SELECT id, name FROM t ORDER BY id").unwrap();
    assert_eq!(out.rows, vec![vec!["1".to_string(), "Alice".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn left_join_preserves_unmatched_outer_rows() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE customers (id INT PRIMARY KEY, name TEXT)")
        .unwrap();
    eng.execute("CREATE TABLE accounts (id INT PRIMARY KEY, customer_id INT, balance INT)")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (1, 'Alice')")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (4, 'Dave')")
        .unwrap();
    eng.execute("INSERT INTO accounts VALUES (10, 1, 5000)")
        .unwrap();
    let out = eng
        .execute(
            "SELECT c.name, a.balance FROM customers c \
             LEFT JOIN accounts a ON c.id = a.customer_id ORDER BY c.name",
        )
        .unwrap();
    assert_eq!(out.rows.len(), 2);
    assert_eq!(out.rows[0], vec!["Alice".to_string(), "5000".to_string()]);
    assert_eq!(out.rows[1], vec!["Dave".to_string(), "[NULL]".to_string()]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn having_count_star_filters_groups() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE customers (id INT PRIMARY KEY, tier TEXT)")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (1, 'gold')")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (2, 'gold')")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (3, 'silver')")
        .unwrap();
    let out = eng
        .execute(
            "SELECT tier, COUNT(*) AS n FROM customers GROUP BY tier HAVING COUNT(*) > 1 ORDER BY tier",
        )
        .unwrap();
    assert_eq!(out.rows, vec![vec!["gold".to_string(), "2".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn exists_select_one_correlated() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE customers (id INT PRIMARY KEY, name TEXT)")
        .unwrap();
    eng.execute("CREATE TABLE accounts (id INT PRIMARY KEY, customer_id INT)")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (1, 'Alice')")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (4, 'Dave')")
        .unwrap();
    eng.execute("INSERT INTO accounts VALUES (10, 1)").unwrap();
    let out = eng
        .execute(
            "SELECT name FROM customers c \
             WHERE NOT EXISTS (SELECT 1 FROM accounts a WHERE a.customer_id = c.id) \
             ORDER BY name",
        )
        .unwrap();
    assert_eq!(out.rows, vec![vec!["Dave".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn in_subquery_with_inner_where() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE customers (id INT PRIMARY KEY, name TEXT)")
        .unwrap();
    eng.execute("CREATE TABLE accounts (id INT PRIMARY KEY, customer_id INT, balance INT)")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (1, 'Alice')")
        .unwrap();
    eng.execute("INSERT INTO customers VALUES (2, 'Bob')")
        .unwrap();
    eng.execute("INSERT INTO accounts VALUES (10, 1, 5000)")
        .unwrap();
    eng.execute("INSERT INTO accounts VALUES (11, 2, 100)")
        .unwrap();
    let out = eng
        .execute(
            "SELECT name FROM customers \
             WHERE id IN (SELECT customer_id FROM accounts WHERE balance > 1000) \
             ORDER BY name",
        )
        .unwrap();
    assert_eq!(out.rows, vec![vec!["Alice".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn select_in_open_transaction_uses_schema() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE accounts (id TEXT, balance INT)")
        .unwrap();
    eng.execute("INSERT INTO accounts VALUES ('a1', 100)")
        .unwrap();
    eng.execute_session(1, "BEGIN").unwrap();
    let out = eng.execute_session(1, "SELECT * FROM accounts").unwrap();
    assert_eq!(out.rows.len(), 1);
    eng.execute_session(1, "ROLLBACK").unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
