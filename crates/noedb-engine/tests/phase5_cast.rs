//! Phase 5 casts and extended types (Week 42).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-cast-{}-{}-{}",
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
fn cast_string_to_int() {
    let (eng, dir) = temp_engine();
    let out = eng.execute("SELECT CAST('42' AS INT) AS n").unwrap();
    assert_eq!(out.rows[0][0], "42");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cast_column_to_int_filter() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("t", "1", "id", b"10").unwrap();
    eng.put_row_default("t", "2", "id", b"20").unwrap();

    let out = eng
        .execute("SELECT id FROM t WHERE CAST(id AS INT) > 15")
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["20"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn int_float_equality_coercion() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("t", "1", "x", b"1").unwrap();

    let out = eng.execute("SELECT x FROM t WHERE x = 1.0").unwrap();
    assert_eq!(out.rows.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn create_table_extended_types() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE ev (d DATE, ts TIMESTAMP, body TEXT)")
        .unwrap();
    eng.put_row_default("ev", "1", "d", b"2026-05-20").unwrap();
    eng.put_row_default("ev", "1", "body", b"hello").unwrap();

    let out = eng
        .execute("SELECT body FROM ev WHERE d = '2026-05-20'")
        .unwrap();
    assert_eq!(out.rows[0][0], "hello");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cast_to_text() {
    let (eng, dir) = temp_engine();
    let out = eng.execute("SELECT CAST(7 AS TEXT) AS s").unwrap();
    assert_eq!(out.rows[0][0], "7");
    let _ = std::fs::remove_dir_all(dir);
}
