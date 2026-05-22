//! Phase 2 MVCC + transaction SQL integration tests.

use std::sync::Arc;

use noedb_engine::{LocalEngine, DEFAULT_SESSION};

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase2-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn begin_commit_sql() {
    let (eng, dir) = temp_engine();
    eng.execute("BEGIN").unwrap();
    eng.put_row_default("accounts", "1", "balance", b"500").unwrap();
    eng.execute("COMMIT").unwrap();
    assert!(!eng.txn().in_txn(DEFAULT_SESSION));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn read_your_writes_in_txn() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("items", "1", "name", b"before").unwrap();
    eng.execute("BEGIN").unwrap();
    eng.put_row_default("items", "1", "name", b"after").unwrap();
    let rows = eng.execute("SELECT name FROM items").unwrap().rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][0], "after");
    eng.execute("ROLLBACK").unwrap();
    let rows = eng.execute("SELECT name FROM items").unwrap().rows;
    assert_eq!(rows[0][0], "before");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parse_begin_commit_rollback() {
    assert!(noedb_parser::parse("BEGIN").is_ok());
    assert!(noedb_parser::parse("COMMIT").is_ok());
    assert!(noedb_parser::parse("ROLLBACK").is_ok());
}
