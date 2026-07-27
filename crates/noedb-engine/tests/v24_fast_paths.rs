//! Equivalence tests for the v2.4 fast paths.
//!
//! The streaming aggregate path (`stream_agg`) and the primary-key point
//! lookup (`PkLookup`) must produce exactly the same results as the
//! generic materializing executor. Every test here asserts observable SQL
//! semantics, so a fast path that diverges fails loudly.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use noedb_engine::LocalEngine;

fn temp_engine(tag: &str) -> (Arc<LocalEngine>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "noedb-v24-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

fn seed_emp(engine: &LocalEngine) {
    engine
        .execute("CREATE TABLE emp (id INT PRIMARY KEY, dept TEXT, salary INT)")
        .unwrap();
    engine
        .execute(
            "INSERT INTO emp VALUES \
             (1, 'eng', 100), (2, 'eng', 200), (3, 'ops', 300), \
             (4, 'ops', 400), (5, 'hr', 500)",
        )
        .unwrap();
}

// ---------------------------------------------------------------- streaming aggregates

#[test]
fn count_star_matches_row_count() {
    let (engine, dir) = temp_engine("count");
    seed_emp(&engine);
    let rows = engine.execute("SELECT COUNT(*) FROM emp").unwrap();
    assert_eq!(rows.rows, vec![vec!["5".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn count_star_on_empty_table_is_zero() {
    let (engine, dir) = temp_engine("empty");
    engine
        .execute("CREATE TABLE nobody (id INT PRIMARY KEY, x INT)")
        .unwrap();
    let rows = engine.execute("SELECT COUNT(*) FROM nobody").unwrap();
    assert_eq!(rows.rows, vec![vec!["0".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn streaming_aggregates_match_hand_computed_values() {
    let (engine, dir) = temp_engine("aggs");
    seed_emp(&engine);
    let rows = engine
        .execute(
            "SELECT COUNT(*), SUM(salary), AVG(salary), MIN(salary), MAX(salary) \
             FROM emp",
        )
        .unwrap();
    assert_eq!(rows.rows.len(), 1);
    assert_eq!(rows.rows[0][0], "5");
    assert_eq!(rows.rows[0][1], "1500");
    assert_eq!(rows.rows[0][2], "300");
    assert_eq!(rows.rows[0][3], "100");
    assert_eq!(rows.rows[0][4], "500");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn streaming_aggregate_honors_where_predicate() {
    let (engine, dir) = temp_engine("filter");
    seed_emp(&engine);
    let rows = engine
        .execute("SELECT COUNT(*) FROM emp WHERE salary > 250")
        .unwrap();
    assert_eq!(rows.rows, vec![vec!["3".to_string()]]);
    // Predicate excluding everything still yields COUNT = 0.
    let rows = engine
        .execute("SELECT COUNT(*) FROM emp WHERE salary > 9999")
        .unwrap();
    assert_eq!(rows.rows, vec![vec!["0".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn streaming_group_by_matches_generic_semantics() {
    let (engine, dir) = temp_engine("group");
    seed_emp(&engine);
    let rows = engine
        .execute("SELECT dept, COUNT(*), SUM(salary) FROM emp GROUP BY dept ORDER BY dept")
        .unwrap();
    let got: Vec<Vec<String>> = rows.rows;
    assert_eq!(
        got,
        vec![
            vec!["eng".to_string(), "2".to_string(), "300".to_string()],
            vec!["hr".to_string(), "1".to_string(), "500".to_string()],
            vec!["ops".to_string(), "2".to_string(), "700".to_string()],
        ]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn count_sees_dml_immediately() {
    let (engine, dir) = temp_engine("dml");
    seed_emp(&engine);
    engine
        .execute("DELETE FROM emp WHERE dept = 'ops'")
        .unwrap();
    let rows = engine.execute("SELECT COUNT(*) FROM emp").unwrap();
    assert_eq!(rows.rows, vec![vec!["3".to_string()]]);

    engine
        .execute("UPDATE emp SET salary = 1000 WHERE id = 5")
        .unwrap();
    let rows = engine.execute("SELECT MAX(salary) FROM emp").unwrap();
    assert_eq!(rows.rows, vec![vec!["1000".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------- primary-key point lookup

#[test]
fn pk_lookup_is_planned_and_returns_the_row() {
    let (engine, dir) = temp_engine("pk");
    seed_emp(&engine);
    let plan = engine.explain("SELECT dept FROM emp WHERE id = 3").unwrap();
    assert!(plan.contains("PkLookup"), "expected PkLookup in: {plan}");

    let rows = engine
        .execute("SELECT dept, salary FROM emp WHERE id = 3")
        .unwrap();
    assert_eq!(rows.rows, vec![vec!["ops".to_string(), "300".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pk_lookup_missing_key_yields_no_rows() {
    let (engine, dir) = temp_engine("pkmiss");
    seed_emp(&engine);
    let rows = engine
        .execute("SELECT dept FROM emp WHERE id = 999")
        .unwrap();
    assert!(rows.rows.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn non_pk_equality_does_not_use_pk_lookup() {
    let (engine, dir) = temp_engine("nonpk");
    seed_emp(&engine);
    let plan = engine
        .explain("SELECT id FROM emp WHERE salary = 300")
        .unwrap();
    assert!(!plan.contains("PkLookup"), "unexpected PkLookup in: {plan}");
    let rows = engine
        .execute("SELECT id FROM emp WHERE salary = 300")
        .unwrap();
    assert_eq!(rows.rows, vec![vec!["3".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn pk_lookup_sees_update_and_delete() {
    let (engine, dir) = temp_engine("pkdml");
    seed_emp(&engine);
    engine
        .execute("UPDATE emp SET dept = 'sec' WHERE id = 1")
        .unwrap();
    let rows = engine.execute("SELECT dept FROM emp WHERE id = 1").unwrap();
    assert_eq!(rows.rows, vec![vec!["sec".to_string()]]);

    engine.execute("DELETE FROM emp WHERE id = 1").unwrap();
    let rows = engine.execute("SELECT dept FROM emp WHERE id = 1").unwrap();
    assert!(rows.rows.is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn drop_table_purges_rows() {
    let (engine, dir) = temp_engine("purge");
    seed_emp(&engine);
    engine.execute("DROP TABLE emp").unwrap();
    engine
        .execute("CREATE TABLE emp (id INT PRIMARY KEY, dept TEXT, salary INT)")
        .unwrap();
    // A dropped table must not resurrect its rows on re-creation.
    let rows = engine.execute("SELECT COUNT(*) FROM emp").unwrap();
    assert_eq!(rows.rows, vec![vec!["0".to_string()]]);
    engine
        .execute("INSERT INTO emp VALUES (9, 'new', 42)")
        .unwrap();
    let rows = engine.execute("SELECT dept FROM emp WHERE id = 9").unwrap();
    assert_eq!(rows.rows, vec![vec!["new".to_string()]]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn drop_table_clears_stale_pk_marker() {
    let (engine, dir) = temp_engine("stale");
    engine
        .execute("CREATE TABLE t (id INT PRIMARY KEY, name TEXT)")
        .unwrap();
    engine.execute("INSERT INTO t VALUES (5, 'old')").unwrap();
    engine.execute("DROP TABLE t").unwrap();

    // Re-create with a different layout: `id` is no longer the row key.
    engine
        .execute("CREATE TABLE t (name TEXT PRIMARY KEY, id INT)")
        .unwrap();
    engine.execute("INSERT INTO t VALUES ('alpha', 5)").unwrap();

    // A stale marker would route this through get(row_id=5) and miss.
    let rows = engine.execute("SELECT name FROM t WHERE id = 5").unwrap();
    assert_eq!(rows.rows, vec![vec!["alpha".to_string()]]);
    let plan = engine.explain("SELECT name FROM t WHERE id = 5").unwrap();
    assert!(!plan.contains("PkLookup"), "stale marker survived: {plan}");
    let _ = std::fs::remove_dir_all(dir);
}
