//! Phase 5 window functions (Week 37).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use noedb_ast::{Expr, WindowFunc};
use noedb_engine::LocalEngine;

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn row_number_over_order_by() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("scores", "a", "id", b"3").unwrap();
    eng.put_row_default("scores", "b", "id", b"1").unwrap();
    eng.put_row_default("scores", "c", "id", b"2").unwrap();

    let out = eng
        .execute("SELECT id, ROW_NUMBER() OVER (ORDER BY id) AS rn FROM scores")
        .unwrap();
    assert_eq!(out.columns, vec!["id", "rn"]);
    assert_eq!(out.rows.len(), 3);
    assert_eq!(out.rows[0], vec!["1".to_string(), "1".to_string()]);
    assert_eq!(out.rows[1], vec!["2".to_string(), "2".to_string()]);
    assert_eq!(out.rows[2], vec!["3".to_string(), "3".to_string()]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rank_ties_over_order_by() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("scores", "a", "id", b"1").unwrap();
    eng.put_row_default("scores", "b", "id", b"1").unwrap();
    eng.put_row_default("scores", "c", "id", b"2").unwrap();

    let out = eng
        .execute("SELECT id, RANK() OVER (ORDER BY id) AS rk FROM scores")
        .unwrap();
    assert_eq!(out.rows.len(), 3);
    assert_eq!(out.rows[0][1], "1");
    assert_eq!(out.rows[1][1], "1");
    assert_eq!(out.rows[2][1], "3");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn dense_rank_partition_by() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("scores", "a", "grp", b"x").unwrap();
    eng.put_row_default("scores", "a", "id", b"2").unwrap();
    eng.put_row_default("scores", "b", "grp", b"x").unwrap();
    eng.put_row_default("scores", "b", "id", b"2").unwrap();
    eng.put_row_default("scores", "c", "grp", b"y").unwrap();
    eng.put_row_default("scores", "c", "id", b"1").unwrap();

    let out = eng
        .execute(
            "SELECT grp, id, DENSE_RANK() OVER (PARTITION BY grp ORDER BY id) AS dr \
             FROM scores",
        )
        .unwrap();
    let x_rows: Vec<_> = out
        .rows
        .iter()
        .filter(|r| r[0] == "x")
        .map(|r| r[2].clone())
        .collect();
    assert_eq!(x_rows, vec!["1", "1"]);
    let y_rows: Vec<_> = out
        .rows
        .iter()
        .filter(|r| r[0] == "y")
        .map(|r| r[2].clone())
        .collect();
    assert_eq!(y_rows, vec!["1"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parse_window_function_ast() {
    let stmt = noedb_parser::parse(
        "SELECT ROW_NUMBER() OVER (PARTITION BY grp ORDER BY id DESC) AS rn FROM t",
    )
    .unwrap();
    let noedb_ast::Statement::Select(sel) = stmt else {
        panic!("expected select");
    };
    let Expr::Function {
        name,
        args,
        over,
        ..
    } = &sel.items[0].expr
    else {
        panic!("expected function");
    };
    assert_eq!(name.value, "ROW_NUMBER");
    assert!(args.is_empty());
    let spec = over.as_ref().expect("OVER spec");
    assert_eq!(spec.partition_by.len(), 1);
    assert_eq!(spec.order_by.len(), 1);
    assert!(!spec.order_by[0].asc);
    assert_eq!(WindowFunc::parse_name(&name.value), Some(WindowFunc::RowNumber));
}

#[test]
fn sum_running_over_order_by() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("t", "a", "id", b"3").unwrap();
    eng.put_row_default("t", "a", "val", b"5").unwrap();
    eng.put_row_default("t", "b", "id", b"1").unwrap();
    eng.put_row_default("t", "b", "val", b"10").unwrap();
    eng.put_row_default("t", "c", "id", b"2").unwrap();
    eng.put_row_default("t", "c", "val", b"20").unwrap();

    let out = eng
        .execute(
            "SELECT id, val, SUM(val) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS rs \
             FROM t",
        )
        .unwrap();
    assert_eq!(out.rows.len(), 3);
    assert_eq!(out.rows[0], vec!["1", "10", "10"]);
    assert_eq!(out.rows[1], vec!["2", "20", "30"]);
    assert_eq!(out.rows[2], vec!["3", "5", "35"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn sum_over_partition_no_order() {
    let (eng, dir) = temp_engine();
    eng.put_row_default("t", "a", "grp", b"x").unwrap();
    eng.put_row_default("t", "a", "val", b"10").unwrap();
    eng.put_row_default("t", "b", "grp", b"x").unwrap();
    eng.put_row_default("t", "b", "val", b"20").unwrap();
    eng.put_row_default("t", "c", "grp", b"y").unwrap();
    eng.put_row_default("t", "c", "val", b"7").unwrap();

    let out = eng
        .execute("SELECT grp, val, SUM(val) OVER (PARTITION BY grp) AS s FROM t")
        .unwrap();
    let x: Vec<_> = out
        .rows
        .iter()
        .filter(|r| r[0] == "x")
        .map(|r| r[2].as_str())
        .collect();
    assert_eq!(x, vec!["30", "30"]);
    let y: Vec<_> = out
        .rows
        .iter()
        .filter(|r| r[0] == "y")
        .map(|r| r[2].as_str())
        .collect();
    assert_eq!(y, vec!["7"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn avg_sliding_frame() {
    let (eng, dir) = temp_engine();
    for (rk, id, val) in [("a", "1", "10"), ("b", "2", "20"), ("c", "3", "30")] {
        eng.put_row_default("t", rk, "id", id.as_bytes()).unwrap();
        eng.put_row_default("t", rk, "val", val.as_bytes()).unwrap();
    }

    let out = eng
        .execute(
            "SELECT id, AVG(val) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) AS av \
             FROM t",
        )
        .unwrap();
    assert_eq!(out.rows.len(), 3);
    assert_eq!(out.rows[0][1], "15");
    assert_eq!(out.rows[1][1], "20");
    assert_eq!(out.rows[2][1], "25");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn parse_sum_over_with_frame() {
    let stmt = noedb_parser::parse(
        "SELECT SUM(v) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM t",
    )
    .unwrap();
    let noedb_ast::Statement::Select(sel) = stmt else {
        panic!();
    };
    let Expr::Function { name, over, .. } = &sel.items[0].expr else {
        panic!();
    };
    assert_eq!(name.value, "SUM");
    let spec = over.as_ref().expect("over");
    let frame = spec.frame.as_ref().expect("frame");
    assert_eq!(frame.start, noedb_ast::FrameBound::UnboundedPreceding);
    assert_eq!(frame.end, noedb_ast::FrameBound::CurrentRow);
}
