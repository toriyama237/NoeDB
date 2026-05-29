//! Query pipeline benchmarks (Week 28): SeqScan vs IndexScan.

#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::significant_drop_tightening,
    clippy::panic
)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use noedb_parser::parse;
use noedb_planner::{
    execute, explain, lower, optimize, plan, ExecutionContext, LogicalPlan, PlanContext,
    SecondaryIndex,
};
use noedb_storage::{LsmConfig, LsmTree, WalSyncMode};

const ROWS: u32 = 100_000;

fn seed_tree() -> (LsmTree, std::path::PathBuf) {
    let dir = std::env::temp_dir().join("noedb-bench-query");
    let _ = std::fs::remove_dir_all(&dir);
    let mut tree = LsmTree::open(
        &dir,
        LsmConfig {
            max_mem_bytes: 64 * 1024 * 1024,
            l0_compaction_trigger: 128,
            wal_sync: WalSyncMode::OnFlush,
            ..Default::default()
        },
    )
    .unwrap();

    for i in 0..ROWS {
        let row = format!("{i}");
        let id = format!("{i}");
        let mut key = b"users".to_vec();
        key.push(0);
        key.extend_from_slice(row.as_bytes());
        key.push(0);
        key.extend_from_slice(b"id");
        tree.put(&key, id.as_bytes()).unwrap();
    }
    SecondaryIndex::build(&mut tree, "users", "id").unwrap();
    (tree, dir)
}

fn point_lookup_sql() -> noedb_ast::Statement {
    parse("SELECT id FROM users WHERE id = '99999'").unwrap()
}

fn bench_seq_scan(c: &mut Criterion) {
    let (tree, _dir) = seed_tree();
    let stmt = point_lookup_sql();
    let logical = plan(&stmt).unwrap();
    let predicate = match logical {
        LogicalPlan::Project { input, .. } => match *input {
            LogicalPlan::Filter { predicate, .. } => predicate,
            other => panic!("unexpected plan {other:?}"),
        },
        other => panic!("unexpected plan {other:?}"),
    };
    let physical = lower(LogicalPlan::Filter {
        input: Box::new(LogicalPlan::Scan {
            table: "users".into(),
            prefix: "users".into(),
        }),
        predicate,
    });
    let ctx = ExecutionContext::single(&tree);

    let mut group = c.benchmark_group("query");
    group.throughput(Throughput::Elements(1));
    group.bench_function("seq_scan_point_lookup", |b| {
        b.iter(|| black_box(execute(physical.clone(), &ctx).unwrap()));
    });
    group.finish();
}

fn bench_index_scan(c: &mut Criterion) {
    let (tree, _dir) = seed_tree();
    let stmt = point_lookup_sql();
    let logical = plan(&stmt).unwrap();
    let pctx = PlanContext::new(&tree);
    let physical = optimize(logical, &pctx);
    let text = explain(&physical, &pctx.stats);
    assert!(text.contains("IndexScan"));
    let ctx = ExecutionContext::single(&tree);

    let mut group = c.benchmark_group("query");
    group.throughput(Throughput::Elements(1));
    group.bench_function("index_scan_point_lookup", |b| {
        b.iter(|| black_box(execute(physical.clone(), &ctx).unwrap()));
    });
    group.finish();
}

criterion_group!(benches, bench_seq_scan, bench_index_scan);
criterion_main!(benches);
