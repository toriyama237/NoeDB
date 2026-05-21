//! Lexer micro-benchmarks.
//!
//! Run with: `cargo bench -p noedb-lexer --bench lexer`.
//!
//! All benches reuse a `Vec` buffer via [`tokenize_into`] so Criterion measures
//! lexing throughput, not allocator churn.

#![allow(missing_docs, clippy::unwrap_used)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use noedb_lexer::tokenize_into;

fn bench_tokenize(c: &mut Criterion, name: &str, input: &'static str) {
    let mut group = c.benchmark_group("lexer");
    #[allow(clippy::cast_possible_truncation)]
    group.throughput(Throughput::Bytes(input.len() as u64));
    group.bench_function(name, |b| {
        b.iter_batched(
            || Vec::with_capacity(32),
            |mut buf| {
                tokenize_into(&mut buf, black_box(input)).unwrap();
                black_box(buf.len());
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn bench_tokenize_select_one(c: &mut Criterion) {
    bench_tokenize(c, "select_one", "SELECT 1");
}

fn bench_tokenize_full_statement(c: &mut Criterion) {
    bench_tokenize(
        c,
        "select_full",
        "SELECT name, age FROM users WHERE active = true AND age > 18 ORDER BY name",
    );
}

fn bench_tokenize_string_heavy(c: &mut Criterion) {
    bench_tokenize(
        c,
        "insert_strings",
        "INSERT INTO logs VALUES ('evt', 'user clicked ''buy''', 1.5)",
    );
}

fn bench_tokenize_where_clause(c: &mut Criterion) {
    bench_tokenize(
        c,
        "select_where",
        "SELECT * FROM users WHERE id = 42 AND active IS NOT NULL",
    );
}

fn bench_tokenize_one_million_tokens(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    let input = "SELECT a, b FROM t WHERE x = 1 AND y = 2";
    #[allow(clippy::cast_possible_truncation)]
    group.throughput(Throughput::Elements(1_000_000));
    group.sample_size(50);
    group.bench_function("one_million_tokens", |b| {
        b.iter_batched(
            || {
                let buf = Vec::with_capacity(32);
                (buf, input)
            },
            |(mut buf, sql)| {
                for _ in 0..50_000 {
                    tokenize_into(&mut buf, black_box(sql)).unwrap();
                }
            },
            BatchSize::LargeInput,
        );
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_tokenize_select_one,
    bench_tokenize_full_statement,
    bench_tokenize_string_heavy,
    bench_tokenize_where_clause,
    bench_tokenize_one_million_tokens
);
criterion_main!(benches);
