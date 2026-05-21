#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

//! Phase 1 security integration tests (Weeks 4–6).

use noedb_engine::LocalEngine;

#[test]
fn prepared_statement_binds_without_interpolation() {
    let dir = std::env::temp_dir().join("noedb-phase1-prep");
    let _ = std::fs::remove_dir_all(&dir);
    let mut eng = LocalEngine::open(&dir).unwrap();
    eng.put_row("docs", "1", "title", b"secret-a").unwrap();
    eng.put_row("docs", "2", "title", b"secret-b").unwrap();

    eng.execute("PREPARE q AS SELECT title FROM docs WHERE title = $1")
        .unwrap();
    let r = eng
        .execute("EXECUTE q ('secret-a')")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0][0], "secret-a");

    let r2 = eng.execute("EXECUTE q ('secret-b')").unwrap();
    assert_eq!(r2.rows[0][0], "secret-b");
}

#[test]
fn rls_isolates_rows_by_role() {
    let dir = std::env::temp_dir().join("noedb-phase1-rls");
    let _ = std::fs::remove_dir_all(&dir);
    let mut eng = LocalEngine::open(&dir).unwrap();
    eng.put_row("t", "1", "owner", b"alice").unwrap();
    eng.put_row("t", "1", "val", b"a1").unwrap();
    eng.put_row("t", "2", "owner", b"bob").unwrap();
    eng.put_row("t", "2", "val", b"b1").unwrap();

    eng.execute("ALTER TABLE t ENABLE ROW LEVEL SECURITY").unwrap();
    eng.execute("CREATE POLICY p ON t USING (owner = CURRENT_USER)")
        .unwrap();

    eng.execute("SET ROLE 'alice'").unwrap();
    let a = eng.execute("SELECT val FROM t").unwrap();
    assert_eq!(a.rows.len(), 1);
    assert_eq!(a.rows[0][0], "a1");

    eng.execute("SET ROLE 'bob'").unwrap();
    let b = eng.execute("SELECT val FROM t").unwrap();
    assert_eq!(b.rows.len(), 1);
    assert_eq!(b.rows[0][0], "b1");
}

#[test]
fn audit_log_records_queries() {
    let dir = std::env::temp_dir().join("noedb-phase1-audit");
    let _ = std::fs::remove_dir_all(&dir);
    let mut eng = LocalEngine::open(&dir).unwrap();
    eng.execute("SELECT 1").unwrap();
    let log = std::fs::read_to_string(dir.join("audit/audit.log")).unwrap();
    assert!(log.contains("SELECT 1"));
    assert!(log.contains("anonymous"));
}
