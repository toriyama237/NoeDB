//! Post-v2 YCSB workload smoke (workloads A/B/C/F at miniature scale).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const RECORDS: u32 = 40;
const OPS: u32 = 80;

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-ycsb-smoke-{}-{}-{}",
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (
        LocalEngine::open_throughput(&dir).unwrap(),
        dir,
    )
}

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

#[test]
fn ycsb_workloads_a_b_c_f_smoke() {
    let (eng, dir) = temp_engine();
    for i in 0..RECORDS {
        eng.put_row_default("users", &row_key(i), "field0", b"v0")
            .unwrap();
    }

    for i in 0..OPS {
        let rk = row_key(i % RECORDS);
        if i % 2 == 0 {
            let _ = read_field(&eng, &rk);
        } else {
            eng.put_row_default("users", &rk, "field0", b"v1").unwrap();
        }
    }

    for i in 0..OPS {
        let rk = row_key(i % RECORDS);
        if i % 20 != 0 {
            let _ = read_field(&eng, &rk);
        } else {
            eng.put_row_default("users", &rk, "field0", b"v2").unwrap();
        }
    }

    for i in 0..OPS {
        let _ = read_field(&eng, &row_key(i % RECORDS));
    }

    for i in 0..OPS {
        if i % 2 == 0 {
            let _ = read_field(&eng, &row_key(i % RECORDS));
        } else {
            eng.put_row_default("users", &format!("new{i}"), "field0", b"x")
                .unwrap();
        }
    }

    let _ = std::fs::remove_dir_all(dir);
}
