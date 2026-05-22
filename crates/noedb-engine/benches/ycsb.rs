//! YCSB-style workloads A/B/C/F on local engine (Phase 3 Week 24).

use std::time::Instant;

use noedb_engine::LocalEngine;

const RECORDS: u32 = 5_000;
const OPS: u32 = 10_000;

fn row_key(i: u32) -> String {
    format!("{i:08}")
}

fn cell_key(table: &str, row: &str, col: &str) -> Vec<u8> {
    let mut k = table.as_bytes().to_vec();
    k.push(0);
    k.extend_from_slice(row.as_bytes());
    k.push(0);
    k.extend_from_slice(col.as_bytes());
    k
}

fn read_field(eng: &LocalEngine, row: &str) -> Vec<u8> {
    eng.store()
        .get_latest(&cell_key("users", row, "field0"))
        .expect("get")
        .unwrap_or_default()
}

fn main() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-ycsb-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut eng = LocalEngine::open_throughput(&dir).expect("open");

    for i in 0..RECORDS {
        eng.put_row("users", &row_key(i), "field0", b"v0")
            .expect("load");
    }

    run_workload("A (50% read / 50% update)", &mut eng, |eng, i| {
        let rk = row_key(i % RECORDS);
        if i % 2 == 0 {
            let _ = read_field(eng, &rk);
        } else {
            eng.put_row("users", &rk, "field0", b"v1").expect("update");
        }
    });

    run_workload("B (95% read / 5% update)", &mut eng, |eng, i| {
        let rk = row_key(i % RECORDS);
        if i % 20 != 0 {
            let _ = read_field(eng, &rk);
        } else {
            eng.put_row("users", &rk, "field0", b"v2").expect("update");
        }
    });

    run_workload("C (100% read)", &mut eng, |eng, i| {
        let _ = read_field(eng, &row_key(i % RECORDS));
    });

    run_workload("F (50% read / 50% insert)", &mut eng, |eng, i| {
        if i % 2 == 0 {
            let _ = read_field(eng, &row_key(i % RECORDS));
        } else {
            let k = format!("new{i}");
            eng.put_row("users", &k, "field0", b"x").expect("insert");
        }
    });

    let _ = std::fs::remove_dir_all(dir);
}

fn run_workload(name: &str, eng: &mut LocalEngine, mut op: impl FnMut(&mut LocalEngine, u32)) {
    let start = Instant::now();
    for i in 0..OPS {
        op(eng, i);
    }
    let elapsed = start.elapsed();
    let ops_s = f64::from(OPS) / elapsed.as_secs_f64();
    println!("ycsb {name}: {OPS} ops in {elapsed:?} ({ops_s:.0} ops/s)");
}
