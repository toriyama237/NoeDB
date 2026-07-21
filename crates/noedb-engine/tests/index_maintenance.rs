//! Secondary indexes must stay consistent with DML (v2.2).
//!
//! Before v2.2, `CREATE INDEX` was a one-shot build: later INSERT / UPDATE /
//! DELETE silently diverged from the index, so `IndexScan` returned stale or
//! incomplete results. These tests pin the maintenance contract.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-idx-maint-{}-{}-{}",
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

fn setup(eng: &LocalEngine) {
    eng.execute("CREATE TABLE users (id INT PRIMARY KEY, city TEXT)")
        .unwrap();
    eng.execute("INSERT INTO users (id, city) VALUES (1, 'paris')")
        .unwrap();
    eng.execute("INSERT INTO users (id, city) VALUES (2, 'lyon')")
        .unwrap();
    eng.execute("CREATE INDEX idx_users_city ON users (city)")
        .unwrap();
    // The predicate below must go through the index.
    let plan = eng
        .explain("SELECT id FROM users WHERE city = 'paris'")
        .unwrap();
    assert!(plan.contains("IndexScan"), "expected IndexScan: {plan}");
}

fn ids_for_city(eng: &LocalEngine, city: &str) -> Vec<String> {
    let res = eng
        .execute(&format!("SELECT id FROM users WHERE city = '{city}'"))
        .unwrap();
    let mut ids: Vec<String> = res.rows.iter().map(|r| r[0].clone()).collect();
    ids.sort();
    ids
}

#[test]
fn index_scan_sees_rows_inserted_after_create_index() {
    let (eng, dir) = temp_engine();
    setup(&eng);

    eng.execute("INSERT INTO users (id, city) VALUES (3, 'paris')")
        .unwrap();

    assert_eq!(ids_for_city(&eng, "paris"), vec!["1", "3"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn index_scan_tracks_updates() {
    let (eng, dir) = temp_engine();
    setup(&eng);

    eng.execute("UPDATE users SET city = 'paris' WHERE id = 2")
        .unwrap();

    assert_eq!(ids_for_city(&eng, "paris"), vec!["1", "2"]);
    assert!(ids_for_city(&eng, "lyon").is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn index_scan_drops_deleted_rows() {
    let (eng, dir) = temp_engine();
    setup(&eng);

    eng.execute("DELETE FROM users WHERE id = 1").unwrap();

    assert!(ids_for_city(&eng, "paris").is_empty());
    assert_eq!(ids_for_city(&eng, "lyon"), vec!["2"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn index_maintenance_applies_on_commit() {
    let (eng, dir) = temp_engine();
    setup(&eng);

    eng.execute("BEGIN").unwrap();
    eng.execute("INSERT INTO users (id, city) VALUES (4, 'paris')")
        .unwrap();
    eng.execute("COMMIT").unwrap();

    assert_eq!(ids_for_city(&eng, "paris"), vec!["1", "4"]);
    let _ = std::fs::remove_dir_all(dir);
}
