//! Lexer micro-benchmarks.
//!
//! Run with: `cargo bench --bench lexer`.
//!
//! These numbers are intentionally tiny on day 1 (the lexer barely
//! understands `SELECT 1`). They exist so that *any* future regression
//! shows up immediately in CI history.

#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use noedb::lexer::tokenize;

fn bench_tokenize_select_one(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    let input = "SELECT 1";
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("select_one", |b| {
        b.iter(|| {
            let _ = tokenize(black_box(input));
        });
    });
    group.finish();
}

fn bench_tokenize_whitespace_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    let input = "   SELECT \t\n   42   ";
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("select_whitespace", |b| {
        b.iter(|| {
            let _ = tokenize(black_box(input));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_tokenize_select_one,
    bench_tokenize_whitespace_heavy
);
criterion_main!(benches);
