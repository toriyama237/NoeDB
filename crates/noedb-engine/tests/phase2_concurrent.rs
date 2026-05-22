//! Phase 2 Week 9 — 100 concurrent sessions on `LocalEngine` (`&self`).

use noedb_engine::LocalEngine;
use rayon::prelude::*;

#[test]
fn hundred_parallel_select_sessions() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-concurrent-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let eng = LocalEngine::open(&dir).unwrap();

    let ok: usize = (0..100u64)
        .into_par_iter()
        .map(|session_id| {
            let out = eng
                .execute_session(session_id + 1, "SELECT 1")
                .expect("select");
            assert_eq!(out.rows[0][0], "1");
            1usize
        })
        .sum();

    assert_eq!(ok, 100);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parallel_begin_put_commit() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-concurrent-txn-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let eng = LocalEngine::open(&dir).unwrap();

    (0..50u64).into_par_iter().for_each(|session_id| {
        let sid = session_id + 1;
        eng.execute_session(sid, "BEGIN").expect("begin");
        eng.put_row(sid, "t", &session_id.to_string(), "v", b"1")
            .expect("put");
        eng.execute_session(sid, "COMMIT").expect("commit");
    });

    assert_eq!(eng.txn().active_count(), 0);
    let _ = std::fs::remove_dir_all(dir);
}
