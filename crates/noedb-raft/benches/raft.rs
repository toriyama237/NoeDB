//! Raft replication throughput (Week 44 target: 10k ops/s @ N=3).

#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use noedb_raft::Cluster;

fn bench_three_node_propose(c: &mut Criterion) {
    let mut group = c.benchmark_group("raft");
    group.throughput(Throughput::Elements(1_000));
    group.sample_size(30);
    group.bench_function("three_node_1k_commands", |b| {
        b.iter_batched(
            || Cluster::new_voters(3).unwrap(),
            |mut cluster| {
                cluster.run_rounds(80).unwrap();
                for i in 0..1_000u32 {
                    let cmd = format!("k:{i}");
                    cluster.propose_on_leader(cmd.into_bytes()).unwrap();
                }
                black_box(cluster.applied_count());
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(benches, bench_three_node_propose);
criterion_main!(benches);
