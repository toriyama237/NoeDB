//! Bulk insert storm for NVMe / LSM stress on bare metal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::print_stderr,
    missing_docs
)]

use std::time::Instant;

use noedb_engine::LocalEngine;

fn main() {
    let records: u64 = std::env::var("NOEDB_STORM_RECORDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    let dir = std::env::var("NOEDB_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("noedb-storm"));

    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open_throughput(&dir).expect("open engine");

    eng.execute("CREATE TABLE bench (id INT PRIMARY KEY, payload VARCHAR)")
        .expect("ddl");

    let start = Instant::now();
    for i in 0..records {
        let sql = format!("INSERT INTO bench VALUES ({i}, 'payload-{i}')");
        eng.execute(&sql).expect("insert");
        if i > 0 && i % 100_000 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let rate = f64::from(i as u32) / elapsed;
            eprintln!("  … {i} rows ({rate:.0} inserts/s)");
        }
    }
    let elapsed = start.elapsed();
    let rate = records as f64 / elapsed.as_secs_f64();
    println!(
        "insert_storm: {records} rows in {elapsed:?} ({rate:.0} inserts/s) data={}",
        dir.display()
    );
}
