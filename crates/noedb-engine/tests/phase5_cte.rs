//! Phase 5 CTEs: `WITH` and `WITH RECURSIVE` (Week 40).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use noedb_engine::LocalEngine;

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-cte-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn with_simple_cte() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();
    eng.put_row_default("users", "3", "id", b"3").unwrap();

    let out = eng
        .execute("WITH picked AS (SELECT id FROM users WHERE id = '2') SELECT id FROM picked")
        .unwrap();
    assert_eq!(out.rows, vec![vec!["2".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_chained_ctes() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("users", "1", "id", b"1").unwrap();
    eng.put_row_default("users", "2", "id", b"2").unwrap();

    let out = eng
        .execute(
            "WITH a AS (SELECT id FROM users WHERE id = '1'), \
             b AS (SELECT id FROM a) \
             SELECT id FROM b",
        )
        .unwrap();
    assert_eq!(out.rows, vec![vec!["1".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_recursive_hierarchy() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("nodes", "a", "id", b"1").unwrap();
    eng.put_row_default("nodes", "a", "parent_id", b"0")
        .unwrap();
    eng.put_row_default("nodes", "b", "id", b"2").unwrap();
    eng.put_row_default("nodes", "b", "parent_id", b"1")
        .unwrap();
    eng.put_row_default("nodes", "c", "id", b"3").unwrap();
    eng.put_row_default("nodes", "c", "parent_id", b"2")
        .unwrap();

    let out = eng
        .execute(
            "WITH RECURSIVE tree AS ( \
               SELECT id FROM nodes WHERE id = '1' \
               UNION ALL \
               SELECT n.id FROM nodes n INNER JOIN tree t ON n.parent_id = t.id \
             ) \
             SELECT id FROM tree",
        )
        .unwrap();
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["1", "2", "3"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_cte_group_by_qualified_column() {
    let (eng, dir) = temp_engine();
    eng.execute("CREATE TABLE agencies (id INT PRIMARY KEY, country TEXT NOT NULL)")
        .unwrap();
    eng.execute("CREATE TABLE employees (id INT PRIMARY KEY, agency_id INT NOT NULL)")
        .unwrap();
    eng.put_row_default("agencies", "1", "id", b"1").unwrap();
    eng.put_row_default("agencies", "1", "country", b"FR")
        .unwrap();
    eng.put_row_default("agencies", "2", "id", b"2").unwrap();
    eng.put_row_default("agencies", "2", "country", b"DE")
        .unwrap();
    eng.put_row_default("employees", "1", "id", b"1").unwrap();
    eng.put_row_default("employees", "1", "agency_id", b"1")
        .unwrap();
    eng.put_row_default("employees", "2", "id", b"2").unwrap();
    eng.put_row_default("employees", "2", "agency_id", b"1")
        .unwrap();
    eng.put_row_default("employees", "3", "id", b"3").unwrap();
    eng.put_row_default("employees", "3", "agency_id", b"2")
        .unwrap();

    let out = eng
        .execute(
            "WITH hc AS (SELECT ag.country, COUNT(e.id) AS n FROM agencies ag \
             INNER JOIN employees e ON e.agency_id = ag.id GROUP BY ag.country) \
             SELECT country, n FROM hc ORDER BY n DESC",
        )
        .unwrap();
    assert_eq!(out.rows.len(), 2);
    assert_eq!(out.rows[0][0], "FR");
    assert_eq!(out.rows[0][1], "2");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn explain_shows_cte_scan() {
    let (eng, dir) = temp_engine();
    let text = eng
        .explain("WITH c AS (SELECT 1 AS x) SELECT x FROM c")
        .unwrap();
    assert!(text.contains("CteScan"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_cte_join_on_parent() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("nodes", "a", "id", b"1").unwrap();
    eng.put_row_default("nodes", "a", "parent_id", b"0")
        .unwrap();
    eng.put_row_default("nodes", "b", "id", b"2").unwrap();
    eng.put_row_default("nodes", "b", "parent_id", b"1")
        .unwrap();

    let out = eng
        .execute(
            "WITH tree AS (SELECT id FROM nodes WHERE id = '1') \
             SELECT n.id FROM nodes n INNER JOIN tree t ON n.parent_id = t.id",
        )
        .unwrap();
    assert_eq!(out.rows, vec![vec!["2".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn with_recursive_anchor_only_via_union() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("nodes", "a", "id", b"1").unwrap();
    eng.put_row_default("nodes", "b", "id", b"2").unwrap();

    let out = eng
        .execute(
            "WITH RECURSIVE tree AS ( \
               SELECT id FROM nodes WHERE id = '1' \
               UNION ALL \
               SELECT id FROM nodes WHERE id = '2' \
             ) \
             SELECT id FROM tree",
        )
        .unwrap();
    let mut ids: Vec<_> = out.rows.into_iter().map(|r| r[0].clone()).collect();
    ids.sort();
    assert_eq!(ids, vec!["1", "2"]);
    let _ = std::fs::remove_dir_all(dir);
}
