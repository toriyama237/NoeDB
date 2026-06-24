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
    eng.execute("INSERT INTO docs VALUES ('d4', NULL)")
        .unwrap();

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
fn select_in_open_transaction_uses_schema() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE accounts (id TEXT, balance INT)")
        .unwrap();
    eng.execute("INSERT INTO accounts VALUES ('a1', 100)")
        .unwrap();
    eng.execute_session(1, "BEGIN").unwrap();
    let out = eng
        .execute_session(1, "SELECT * FROM accounts")
        .unwrap();
    assert_eq!(out.rows.len(), 1);
    eng.execute_session(1, "ROLLBACK").unwrap();
    let _ = std::fs::remove_dir_all(dir);
}
