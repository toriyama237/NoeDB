//! Phase 5 wave 3: HAVING, EXISTS, derived tables, K-NN distance sort.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-w3-{}-{}-{}",
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
fn having_filters_groups() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("sales", "a", "region", b"east").unwrap();
    eng.put_row_default("sales", "a", "amount", b"10").unwrap();
    eng.put_row_default("sales", "b", "region", b"east").unwrap();
    eng.put_row_default("sales", "b", "amount", b"20").unwrap();
    eng.put_row_default("sales", "c", "region", b"west").unwrap();
    eng.put_row_default("sales", "c", "amount", b"100").unwrap();

    let out = eng
        .execute(
            "SELECT region, SUM(CAST(amount AS INT)) AS total \
             FROM sales GROUP BY region HAVING total > 25",
        )
        .unwrap();
    let mut regions: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    regions.sort();
    assert_eq!(regions, vec!["east", "west"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn exists_uncorrelated() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();
    eng.put_row_default("orders", "a", "user_id", b"1").unwrap();

    let out = eng
        .execute("SELECT id FROM users WHERE EXISTS (SELECT user_id FROM orders)")
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["1", "2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn not_exists_correlated() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();
    eng.put_row_default("orders", "a", "user_id", b"1").unwrap();

    let out = eng
        .execute(
            "SELECT id FROM users u \
             WHERE NOT EXISTS (SELECT user_id FROM orders o WHERE o.user_id = u.id)",
        )
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn derived_table_from_subquery() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("items", "a", "id", b"1").unwrap();
    eng.put_row_default("items", "a", "qty", b"5").unwrap();
    eng.put_row_default("items", "b", "id", b"2").unwrap();
    eng.put_row_default("items", "b", "qty", b"1").unwrap();

    let out = eng
        .execute(
            "SELECT id FROM (SELECT id, qty FROM items) AS sub WHERE CAST(qty AS INT) >= 5",
        )
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["1"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn knn_order_by_distance_limit() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE docs (id INT PRIMARY KEY, emb VECTOR(3))")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES (1, '[0,0,0]')")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES (2, '[1,0,0]')")
        .unwrap();
    eng.execute("INSERT INTO docs VALUES (3, '[3,0,0]')")
        .unwrap();

    let out = eng
        .execute("SELECT id FROM docs ORDER BY emb <-> '[0,0,0]' LIMIT 2")
        .unwrap();
    let ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    assert_eq!(ids, vec!["1", "2"]);
    let top = eng.vector_search("docs", "emb", &[0.0, 0.0, 0.0], 2);
    assert_eq!(top[0].0, "1");
    assert_eq!(top[1].0, "2");
    let _ = std::fs::remove_dir_all(dir);
}
