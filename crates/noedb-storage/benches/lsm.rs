//! LSM throughput benchmarks (Week 16).

#![allow(missing_docs, clippy::unwrap_used, clippy::significant_drop_tightening)]

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use noedb_storage::{LsmConfig, LsmTree};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("noedb-bench-{name}-{nanos}"))
}

fn bench_lsm_puts(c: &mut Criterion) {
    let mut group = c.benchmark_group("lsm");
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("ten_k_puts", |b| {
        b.iter(|| {
            let dir = temp_dir("puts");
            let mut tree = LsmTree::open(
                &dir,
                LsmConfig {
                    max_mem_bytes: 256 * 1024,
                    l0_compaction_trigger: 8,
                },
            )
            .unwrap();
            for i in 0..10_000u32 {
                let k = format!("k:{i:05}");
                tree.put(black_box(k.as_bytes()), black_box(b"value"))
                    .unwrap();
            }
            let _ = std::fs::remove_dir_all(dir);
        });
    });
    group.finish();
}

fn bench_lsm_gets(c: &mut Criterion) {
    let dir = temp_dir("gets-setup");
    let mut tree = LsmTree::open(
        &dir,
        LsmConfig {
            max_mem_bytes: 256 * 1024,
            l0_compaction_trigger: 8,
        },
    )
    .unwrap();
    for i in 0..10_000u32 {
        let k = format!("k:{i:05}");
        tree.put(k.as_bytes(), b"value").unwrap();
    }
    drop(tree);

    let mut group = c.benchmark_group("lsm");
    group.throughput(Throughput::Elements(10_000));
    group.bench_function("ten_k_gets", |b| {
        b.iter(|| {
            let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
            for i in 0..10_000u32 {
                let k = format!("k:{i:05}");
                let _ = tree.get(black_box(k.as_bytes())).unwrap();
            }
        });
    });
    group.finish();
    let _ = std::fs::remove_dir_all(dir);
}

criterion_group!(benches, bench_lsm_puts, bench_lsm_gets);
criterion_main!(benches);
