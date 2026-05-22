//! End-to-end pipeline benchmark (Phase 5, Week 48).

#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use noedb_engine::{DistributedEngine, LocalEngine};

fn bench_local_select(c: &mut Criterion) {
    let dir = std::env::temp_dir().join("noedb-bench-local");
    let eng = LocalEngine::open(&dir).unwrap();
    for i in 0..100u64 {
        eng.put_row_default("users", &i.to_string(), "name", b"x")
            .unwrap();
    }
    c.bench_function("engine/local_select_100_rows", |b| {
        b.iter(|| {
            let out = eng.execute("SELECT name FROM users").unwrap();
            black_box(out.rows.len());
        });
    });
    let _ = std::fs::remove_dir_all(dir);
}

fn bench_distributed_select(c: &mut Criterion) {
    let eng = DistributedEngine::new_voters(3).expect("cluster");
    eng.tick(80).expect("tick");
    for i in 0..100u64 {
        eng.seed_leader_row("users", &i.to_string(), "name", b"x")
            .expect("seed");
    }
    c.bench_function("engine/distributed_select_100_rows", |b| {
        b.iter(|| {
            let out = eng.execute("SELECT name FROM users").expect("select");
            black_box(out.rows.len());
        });
    });
}

criterion_group!(benches, bench_local_select, bench_distributed_select);
criterion_main!(benches);
