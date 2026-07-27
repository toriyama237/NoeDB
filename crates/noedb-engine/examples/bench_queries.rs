//! Query micro-benchmark: 50 k packed rows, min-of-N timings.
//!
//! The national audit example measures single cold executions, which are
//! dominated by machine noise (±2×). This bench loops each query and
//! reports the minimum — the reproducible number for optimization work.
//!
//! Run: `cargo run --release -p noedb-engine --example bench_queries`

#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss
)]

use std::time::Instant;

use noedb_engine::LocalEngine;

const ROWS: u32 = 50_000;
const ITERS: usize = 7;

fn main() {
    let dir = std::env::temp_dir().join(format!("noedb-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open_throughput(&dir).expect("open");

    eng.execute(
        "CREATE TABLE emp (id INTEGER PRIMARY KEY, agency_id INTEGER, dept_id INTEGER, \
         salary INTEGER, grade TEXT, name TEXT)",
    )
    .unwrap();

    let t0 = Instant::now();
    for id in 1..=ROWS {
        let row: Vec<(String, Vec<u8>)> = vec![
            ("id".into(), id.to_string().into_bytes()),
            ("agency_id".into(), (id % 80).to_string().into_bytes()),
            ("dept_id".into(), (id % 22).to_string().into_bytes()),
            (
                "salary".into(),
                (30_000 + (id % 9) * 7_000).to_string().into_bytes(),
            ),
            ("grade".into(), format!("G{}", id % 5).into_bytes()),
            ("name".into(), format!("emp-{id}").into_bytes()),
        ];
        eng.put_packed_row("emp", &id.to_string(), &row).unwrap();
    }
    eng.execute("ANALYZE TABLE emp").unwrap();
    println!("load {ROWS} rows: {:.2}s", t0.elapsed().as_secs_f64());

    let queries: &[(&str, &str)] = &[
        ("count_star", "SELECT COUNT(*) AS n FROM emp"),
        (
            "count_where",
            "SELECT COUNT(*) AS n FROM emp WHERE salary > 60000",
        ),
        (
            "group_by",
            "SELECT agency_id, COUNT(*) AS n FROM emp GROUP BY agency_id",
        ),
        (
            "sum_group",
            "SELECT dept_id, SUM(salary) AS m FROM emp GROUP BY dept_id",
        ),
        (
            "avg_where",
            "SELECT AVG(salary) AS a FROM emp WHERE grade = 'G1'",
        ),
        (
            "topk",
            "SELECT name, salary FROM emp ORDER BY salary DESC LIMIT 10",
        ),
        ("point", "SELECT name FROM emp WHERE id = 25000"),
    ];

    println!("{:<12} {:>10} {:>10}", "query", "min(ms)", "med(ms)");
    for (name, sql) in queries {
        let mut times: Vec<f64> = Vec::with_capacity(ITERS);
        for i in 0..ITERS {
            // Unique alias per iteration bypasses the result cache.
            let sql_i = sql.replacen(" AS ", &format!(" AS x{i}_"), 1);
            let sql_i = if sql_i.contains(" AS x") {
                sql_i
            } else {
                format!("{sql} -- {i}")
            };
            let t = Instant::now();
            let r = eng.execute(&sql_i).expect(name);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            assert!(
                !r.rows.is_empty() || name.contains("point"),
                "{name}: empty"
            );
            times.push(ms);
        }
        times.sort_by(f64::total_cmp);
        println!(
            "{:<12} {:>10.2} {:>10.2}",
            name,
            times[0],
            times[times.len() / 2]
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
