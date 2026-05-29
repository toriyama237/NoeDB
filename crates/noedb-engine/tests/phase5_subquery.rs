//! Phase 5 subqueries: `IN (SELECT …)` decorrelation (Week 39).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-sq-{}-{}-{}",
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
fn where_in_uncorrelated_subquery() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();
    eng.put_row_default("users", "3", "id", b"3").unwrap();
    eng.put_row_default("orders", "a", "user_id", b"1").unwrap();
    eng.put_row_default("orders", "b", "user_id", b"3").unwrap();

    let out = eng
        .execute("SELECT id FROM users WHERE id IN (SELECT user_id FROM orders)")
        .unwrap();
    assert_eq!(out.columns, vec!["id"]);
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["1", "3"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn where_not_in_subquery() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();
    eng.put_row_default("orders", "a", "user_id", b"2").unwrap();

    let out = eng
        .execute("SELECT id FROM users WHERE id NOT IN (SELECT user_id FROM orders)")
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["1"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn where_in_correlated_subquery() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("orders", "10", "id", b"10").unwrap();
    eng.put_row_default("orders", "20", "id", b"20").unwrap();
    eng.put_row_default("lines", "a", "order_id", b"10")
        .unwrap();
    eng.put_row_default("lines", "b", "order_id", b"10")
        .unwrap();
    eng.put_row_default("lines", "c", "order_id", b"99")
        .unwrap();

    let out = eng
        .execute(
            "SELECT id FROM orders o \
             WHERE id IN (SELECT order_id FROM lines l WHERE l.order_id = o.id)",
        )
        .unwrap();
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["10"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explain_shows_semi_join() {
    let (eng, dir) = temp_engine();
    let text = eng
        .explain("SELECT id FROM users WHERE id IN (SELECT user_id FROM orders)")
        .unwrap();
    assert!(text.contains("SemiJoin"));
    let _ = std::fs::remove_dir_all(dir);
}
