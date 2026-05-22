//! End-to-end integration tests (Phase 5, Week 45).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_engine::{DistributedEngine, LocalEngine};

#[test]
fn local_select_literal() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-e2e-local-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let eng = LocalEngine::open(&dir).unwrap();
    let out = eng.execute("SELECT 1").unwrap();
    assert_eq!(out.rows.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn distributed_put_and_select() {
    let mut eng = DistributedEngine::new_voters(3).unwrap();
    eng.tick(80).unwrap();
    eng.put_row("users", "1", "name", b"ada").unwrap();
    let out = eng.execute("SELECT name FROM users").unwrap();
    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.rows[0][0], "ada");
}

#[test]
fn distributed_create_index() {
    let mut eng = DistributedEngine::new_voters(3).unwrap();
    eng.tick(80).unwrap();
    eng.put_row("users", "1", "id", b"7").unwrap();
    eng.put_row("users", "1", "name", b"ada").unwrap();
    eng.execute("CREATE INDEX idx ON users (id)").unwrap();
    let plan = eng.explain("SELECT name FROM users WHERE id = '7'").unwrap();
    assert!(plan.contains("IndexScan"));
}
