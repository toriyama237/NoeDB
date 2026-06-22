//! Twenty-query E2E corpus (Phase 5, Week 45).

#![allow(clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use noedb_engine::LocalEngine;

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-corpus-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn corpus_twenty_queries() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "1", "name", b"ada").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();
    eng.put_row_default("users", "2", "name", b"bob").unwrap();
    eng.put_row_default("orders", "9", "user_id", b"1").unwrap();
    eng.put_row_default("orders", "9", "sku", b"book").unwrap();

    let queries = [
        "SELECT 1",
        "SELECT 42",
        "SELECT name FROM users",
        "SELECT id FROM users",
        "SELECT id FROM users WHERE id = '1'",
        "SELECT name FROM users WHERE name = 'ada'",
        "SELECT name FROM users WHERE name = 'bob'",
        "SELECT 1 FROM users",
        "SELECT users.name FROM users",
        "SELECT name FROM users INNER JOIN orders ON users.id = orders.user_id",
        "SELECT name FROM users WHERE id = '1' AND name = 'ada'",
        "SELECT name FROM users WHERE id = '2' OR name = 'ada'",
        "SELECT id FROM users WHERE id = '1'",
        "SELECT name FROM users WHERE id = '2'",
        "SELECT sku FROM orders",
        "SELECT name FROM users WHERE id = '1'",
        "SELECT name FROM users WHERE id = '2'",
        "SELECT 7",
        "SELECT name FROM users",
        "SELECT id FROM users WHERE id = '2'",
    ];

    for (i, sql) in queries.iter().enumerate() {
        eng.execute(sql)
            .unwrap_or_else(|e| panic!("query {i} `{sql}`: {e}"));
    }

    eng.execute("CREATE INDEX idx_users_id ON users (id)")
        .unwrap();
    let plan = eng
        .explain("SELECT name FROM users WHERE id = '1'")
        .unwrap();
    assert!(plan.contains("IndexScan"));

    let _ = std::fs::remove_dir_all(dir);
}
