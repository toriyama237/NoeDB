//! LSM throughput benchmarks (Week 16+).
//!
//! Setup is outside the timed loop; timed work is pure puts/gets.

#![allow(missing_docs, clippy::unwrap_used, clippy::significant_drop_tightening)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use noedb_storage::{LsmConfig, LsmTree, WalSyncMode};

fn bench_lsm_puts(c: &mut Criterion) {
    let dir = std::env::temp_dir().join("noedb-bench-lsm-puts");
    let _ = std::fs::remove_dir_all(&dir);

    let keys: Vec<Vec<u8>> = (0..10_000u32).map(|i| format!("k:{i:05}").into_bytes()).collect();
    let entries: Vec<(&[u8], &[u8])> = keys.iter().map(|k| (k.as_slice(), b"value" as &[u8])).collect();

    let mut group = c.benchmark_group("lsm");
    group.throughput(Throughput::Elements(10_000));
    group.sample_size(50);
    group.bench_function("ten_k_puts", |b| {
        b.iter_batched(
            || {
                let _ = std::fs::remove_dir_all(&dir);
                LsmTree::open(
                    &dir,
                    LsmConfig {
                        max_mem_bytes: 16 * 1024 * 1024,
                        l0_compaction_trigger: 64,
                        wal_sync: WalSyncMode::OnFlush,
                        ..Default::default()
                    },
                )
                .unwrap()
            },
            |mut tree| {
                tree.put_batch(black_box(&entries)).unwrap();
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

fn bench_lsm_gets(c: &mut Criterion) {
    let dir = std::env::temp_dir().join("noedb-bench-lsm-gets");
    let _ = std::fs::remove_dir_all(&dir);

    let keys: Vec<Vec<u8>> = (0..10_000u32).map(|i| format!("k:{i:05}").into_bytes()).collect();
    {
        let mut tree = LsmTree::open(&dir, LsmConfig::throughput()).unwrap();
        let entries: Vec<(&[u8], &[u8])> =
            keys.iter().map(|k| (k.as_slice(), b"value" as &[u8])).collect();
        tree.put_batch(&entries).unwrap();
    }

    let mut group = c.benchmark_group("lsm");
    group.throughput(Throughput::Elements(10_000));
    group.sample_size(50);
    group.bench_function("ten_k_gets", |b| {
        b.iter(|| {
            let tree = LsmTree::open(&dir, LsmConfig::throughput()).unwrap();
            for key in &keys {
                let _ = black_box(tree.get(key).unwrap());
            }
        });
    });
    group.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

criterion_group!(benches, bench_lsm_puts, bench_lsm_gets);
criterion_main!(benches);
