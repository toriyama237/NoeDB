//! Lexer micro-benchmarks.
//!
//! Run with: `cargo bench -p noedb-lexer --bench lexer`.

#![allow(missing_docs)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use noedb_lexer::tokenize;

fn bench_tokenize_select_one(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    let input = "SELECT 1";
    #[allow(clippy::cast_possible_truncation)]
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("select_one", |b| {
        b.iter(|| {
            let _ = tokenize(black_box(input));
        });
    });
    group.finish();
}

fn bench_tokenize_full_statement(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    let input = "SELECT name, age FROM users WHERE active = true AND age > 18 ORDER BY name";
    #[allow(clippy::cast_possible_truncation)]
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("select_full", |b| {
        b.iter(|| {
            let _ = tokenize(black_box(input));
        });
    });
    group.finish();
}

fn bench_tokenize_string_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    let input = "INSERT INTO logs VALUES ('evt', 'user clicked ''buy''', 1.5)";
    #[allow(clippy::cast_possible_truncation)]
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function("insert_strings", |b| {
        b.iter(|| {
            let _ = tokenize(black_box(input));
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_tokenize_select_one,
    bench_tokenize_full_statement,
    bench_tokenize_string_heavy
);
criterion_main!(benches);
