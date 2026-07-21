//! End-to-end coverage of the packed row layout (v2.3).
//!
//! SQL `INSERT` writes one record per row; `UPDATE` upgrades legacy
//! cell-based rows; `DELETE` removes both layouts; the bulk
//! `put_packed_row` API is visible to SQL immediately.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use noedb_engine::LocalEngine;

fn temp_engine(tag: &str) -> (Arc<LocalEngine>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "noedb-packed-{tag}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn sql_crud_round_trip_on_packed_layout() {
    let (engine, dir) = temp_engine("crud");
    engine
        .execute("CREATE TABLE staff (id INT PRIMARY KEY, name TEXT, salary INT)")
        .unwrap();
    engine
        .execute("INSERT INTO staff VALUES (1, 'Ada', 4200), (2, 'Grace', 5100)")
        .unwrap();

    let rows = engine
        .execute("SELECT name FROM staff WHERE salary > 5000")
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][0], "Grace");

    engine
        .execute("UPDATE staff SET salary = 6000 WHERE id = 1")
        .unwrap();
    let rows = engine
        .execute("SELECT COUNT(*) FROM staff WHERE salary >= 5100")
        .unwrap();
    assert_eq!(rows.rows[0][0], "2");

    engine.execute("DELETE FROM staff WHERE id = 2").unwrap();
    let rows = engine.execute("SELECT COUNT(*) FROM staff").unwrap();
    assert_eq!(rows.rows[0][0], "1");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn bulk_put_packed_row_is_visible_to_sql() {
    let (engine, dir) = temp_engine("bulk");
    engine
        .execute("CREATE TABLE metrics (id INT PRIMARY KEY, val INT)")
        .unwrap();
    for i in 0..500u32 {
        engine
            .put_packed_row(
                "metrics",
                &i.to_string(),
                &[
                    ("id".to_string(), i.to_string().into_bytes()),
                    ("val".to_string(), (i * 2).to_string().into_bytes()),
                ],
            )
            .unwrap();
    }
    let rows = engine.execute("SELECT COUNT(*) FROM metrics").unwrap();
    assert_eq!(rows.rows[0][0], "500");
    let rows = engine
        .execute("SELECT val FROM metrics WHERE id = 250")
        .unwrap();
    assert_eq!(rows.rows[0][0], "500");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn update_upgrades_legacy_cell_rows() {
    let (engine, dir) = temp_engine("upgrade");
    engine
        .execute("CREATE TABLE legacy (id INT PRIMARY KEY, status TEXT)")
        .unwrap();
    // Row written with the historical cell-per-column API.
    engine.put_row_default("legacy", "7", "id", b"7").unwrap();
    engine
        .put_row_default("legacy", "7", "status", b"old")
        .unwrap();

    engine
        .execute("UPDATE legacy SET status = 'new' WHERE id = 7")
        .unwrap();
    let rows = engine
        .execute("SELECT status FROM legacy WHERE id = 7")
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][0], "new");

    engine.execute("DELETE FROM legacy WHERE id = 7").unwrap();
    let rows = engine.execute("SELECT COUNT(*) FROM legacy").unwrap();
    assert_eq!(rows.rows[0][0], "0");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn transactional_insert_commits_packed_rows() {
    let (engine, dir) = temp_engine("txn");
    engine
        .execute("CREATE TABLE acc (id INT PRIMARY KEY, bal INT)")
        .unwrap();
    engine.execute("BEGIN").unwrap();
    engine.execute("INSERT INTO acc VALUES (1, 100)").unwrap();
    engine.execute("INSERT INTO acc VALUES (2, 250)").unwrap();
    engine.execute("COMMIT").unwrap();

    let rows = engine.execute("SELECT SUM(bal) FROM acc").unwrap();
    assert_eq!(rows.rows[0][0], "350");

    engine.execute("BEGIN").unwrap();
    engine.execute("DELETE FROM acc WHERE id = 1").unwrap();
    engine.execute("ROLLBACK").unwrap();
    let rows = engine.execute("SELECT COUNT(*) FROM acc").unwrap();
    assert_eq!(rows.rows[0][0], "2");
    let _ = std::fs::remove_dir_all(dir);
}
