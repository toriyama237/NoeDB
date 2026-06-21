#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

//! Phase 1 security integration tests (Weeks 4–6).

use noedb_engine::LocalEngine;

#[test]
fn prepared_statement_binds_without_interpolation() {
    let dir = std::env::temp_dir().join("noedb-phase1-prep");
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open(&dir).unwrap();
    eng.put_row_default("docs", "1", "title", b"secret-a")
        .unwrap();
    eng.put_row_default("docs", "2", "title", b"secret-b")
        .unwrap();

    eng.execute("PREPARE q AS SELECT title FROM docs WHERE title = $1")
        .unwrap();
    let r = eng.execute("EXECUTE q ('secret-a')").unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0][0], "secret-a");

    let r2 = eng.execute("EXECUTE q ('secret-b')").unwrap();
    assert_eq!(r2.rows[0][0], "secret-b");
}

#[test]
fn rls_isolates_rows_by_role() {
    let dir = std::env::temp_dir().join("noedb-phase1-rls");
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open(&dir).unwrap();
    eng.put_row_default("t", "1", "owner", b"alice").unwrap();
    eng.put_row_default("t", "1", "val", b"a1").unwrap();
    eng.put_row_default("t", "2", "owner", b"bob").unwrap();
    eng.put_row_default("t", "2", "val", b"b1").unwrap();

    eng.execute("ALTER TABLE t ENABLE ROW LEVEL SECURITY")
        .unwrap();
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
fn ddl_requires_admin_role() {
    let dir = std::env::temp_dir().join("noedb-phase1-ddl-guard");
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open(&dir).unwrap();
    eng.execute("SET ROLE 'anonymous'").unwrap();
    assert!(eng
        .execute("CREATE TABLE secret (id INT PRIMARY KEY)")
        .is_err());
    assert!(eng.execute("SET ROLE 'admin'").is_err());
    let dir2 = std::env::temp_dir().join("noedb-phase1-ddl-guard-admin");
    let _ = std::fs::remove_dir_all(&dir2);
    let admin = LocalEngine::open(&dir2).unwrap();
    admin
        .execute("CREATE TABLE secret (id INT PRIMARY KEY)")
        .unwrap();
}

#[test]
fn audit_hash_chain_is_valid() {
    let dir = std::env::temp_dir().join("noedb-phase1-audit-chain");
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open(&dir).unwrap();
    eng.execute("SELECT 1").unwrap();
    eng.execute("SELECT 2").unwrap();
    let log = noedb_engine::AuditLog::open(&dir).unwrap();
    log.verify_chain().unwrap();
}

#[test]
fn at_rest_encryption_round_trip_and_no_plaintext_on_disk() {
    let dir = std::env::temp_dir().join("noedb-phase1-encrypt");
    let _ = std::fs::remove_dir_all(&dir);
    std::env::set_var("NOEDB_DATA_KEY", "phase1-test-master-key");
    let eng = LocalEngine::open(&dir).unwrap();
    assert!(eng.encryption_enabled());

    let secret = b"balance=4242.00";
    eng.put_row_encrypted(noedb_engine::DEFAULT_SESSION, "accounts", "1", "bal", secret)
        .unwrap();

    let got = eng
        .get_row_decrypted("accounts", "1", "bal")
        .unwrap()
        .unwrap();
    assert_eq!(got, secret);

    // Force a flush so bytes hit an SST, then scan the data dir for plaintext.
    drop(eng);
    let mut found_plaintext = false;
    for entry in walk(&dir) {
        if let Ok(bytes) = std::fs::read(&entry) {
            if bytes
                .windows(secret.len())
                .any(|w| w == secret)
            {
                found_plaintext = true;
            }
        }
    }
    std::env::remove_var("NOEDB_DATA_KEY");
    assert!(!found_plaintext, "plaintext secret leaked to disk");
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn audit_log_records_queries() {
    let dir = std::env::temp_dir().join("noedb-phase1-audit");
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open(&dir).unwrap();
    eng.execute("SET ROLE 'anonymous'").unwrap();
    eng.execute("SELECT 1").unwrap();
    let log = std::fs::read_to_string(dir.join("audit/audit.log")).unwrap();
    assert!(log.contains("SELECT 1"));
    assert!(log.contains("anonymous"));
}
