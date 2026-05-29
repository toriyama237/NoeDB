//! Phase 5 set operations: `UNION` / `INTERSECT` / `EXCEPT` (Week 41).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-setops-{}-{}-{}",
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
fn union_all_preserves_duplicates() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("a", "1", "id", b"1").unwrap();
    eng.put_row_default("a", "2", "id", b"2").unwrap();
    eng.put_row_default("b", "3", "id", b"2").unwrap();

    let out = eng
        .execute("SELECT id FROM a UNION ALL SELECT id FROM b")
        .unwrap();
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["1", "2", "2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn union_distinct_dedupes() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("a", "1", "id", b"1").unwrap();
    eng.put_row_default("a", "2", "id", b"2").unwrap();
    eng.put_row_default("b", "3", "id", b"2").unwrap();

    let out = eng
        .execute("SELECT id FROM a UNION SELECT id FROM b")
        .unwrap();
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["1", "2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn intersect_common_rows() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("a", "1", "id", b"1").unwrap();
    eng.put_row_default("a", "2", "id", b"2").unwrap();
    eng.put_row_default("b", "3", "id", b"2").unwrap();
    eng.put_row_default("b", "4", "id", b"3").unwrap();

    let out = eng
        .execute("SELECT id FROM a INTERSECT SELECT id FROM b")
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn except_left_minus_right() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("a", "1", "id", b"1").unwrap();
    eng.put_row_default("a", "2", "id", b"2").unwrap();
    eng.put_row_default("b", "3", "id", b"2").unwrap();

    let out = eng
        .execute("SELECT id FROM a EXCEPT SELECT id FROM b")
        .unwrap();
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["1"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explain_shows_set_op() {
    let (eng, dir) = temp_engine();
    let text = eng
        .explain("SELECT id FROM a UNION SELECT id FROM b")
        .unwrap();
    assert!(text.contains("SetOp(UNION)"));
    let _ = std::fs::remove_dir_all(dir);
}
