//! Parsed-statement cache behaviour (v2.2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-stmtcache-{}-{}-{}",
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
fn repeated_statement_hits_ast_cache() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE t (id INT PRIMARY KEY, v TEXT)")
        .unwrap();
    eng.execute("INSERT INTO t (id, v) VALUES (1, 'a')")
        .unwrap();

    let (h0, _) = eng.statement_cache_stats();
    for _ in 0..5 {
        let res = eng.execute("SELECT v FROM t WHERE id = 1").unwrap();
        assert_eq!(res.rows, vec![vec!["a".to_string()]]);
    }
    let (h1, _) = eng.statement_cache_stats();
    assert!(h1 >= h0 + 4, "expected AST cache hits, got {h0} -> {h1}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cached_ast_still_sees_fresh_data() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE t (id INT PRIMARY KEY, v TEXT)")
        .unwrap();
    eng.execute("INSERT INTO t (id, v) VALUES (1, 'before')")
        .unwrap();

    let q = "SELECT v FROM t WHERE id = 1";
    assert_eq!(
        eng.execute(q).unwrap().rows,
        vec![vec!["before".to_string()]]
    );

    // The AST cache must not freeze results: only parsing is reused.
    eng.execute("UPDATE t SET v = 'after' WHERE id = 1")
        .unwrap();
    assert_eq!(
        eng.execute(q).unwrap().rows,
        vec![vec!["after".to_string()]]
    );
    let _ = std::fs::remove_dir_all(dir);
}
