//! Transactional SQL DML and HNSW fast-path tests.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-txn-dml-{}-{}-{}",
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
fn insert_update_delete_in_transaction() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE t (id INT PRIMARY KEY, v VARCHAR)").unwrap();
    eng.execute("INSERT INTO t VALUES (1, 'old')").unwrap();

    eng.execute("BEGIN").unwrap();
    eng.execute("INSERT INTO t VALUES (2, 'new')").unwrap();
    eng.execute("UPDATE t SET v = 'changed' WHERE id = '1'").unwrap();
    let rows = eng.execute("SELECT id FROM t ORDER BY id").unwrap();
    assert_eq!(rows.rows.len(), 2);
    eng.execute("DELETE FROM t WHERE id = '1'").unwrap();
    let rows = eng.execute("SELECT id FROM t").unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][0], "2");
    eng.execute("COMMIT").unwrap();

    let rows = eng.execute("SELECT id FROM t ORDER BY id").unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][0], "2");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn hnsw_fast_path_matches_sort() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs (id INT PRIMARY KEY, emb VECTOR(3))")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES (1, '[0,0,0]')")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES (2, '[1,0,0]')")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES (3, '[3,0,0]')")
        .unwrap();

    let sorted = eng
        .execute("SELECT id FROM docs ORDER BY emb <-> '[0,0,0]' LIMIT 2")
        .unwrap();
    let ids: Vec<_> = sorted.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["1", "2"]);
    let _ = std::fs::remove_dir_all(dir);
}
