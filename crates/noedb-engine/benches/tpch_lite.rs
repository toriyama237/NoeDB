//! TPC-H **lite** analytical workload (Phase 5 Week 44).
//!
//! Subset of TPC-H-style queries using the Phase 5 SQL surface (joins, `IN`
//! subqueries, CTEs, `UNION`, windows, `CAST`, indexes + `ANALYZE`).
//!
//! ```bash
//! cargo bench -p noedb-engine --bench tpch_lite
//! ```

#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::significant_drop_tightening,
    clippy::cast_precision_loss
)]

use std::hint::black_box;
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use noedb_engine::LocalEngine;

/// Scale factor (lite): row counts for each table.
const CUSTOMERS: u32 = 800;
const ORDERS: u32 = 3_000;
const LINEITEMS: u32 = 9_000;
const PARTS: u32 = 400;

const Q03: &str = "\
SELECT l.l_orderkey, l.l_extendedprice, o.o_orderdate \
FROM lineitem l \
INNER JOIN orders o ON l.l_orderkey = o.o_orderkey \
INNER JOIN customer c ON o.o_custkey = c.c_custkey \
WHERE c.c_mktsegment = 'BUILDING' AND o.o_orderdate < '1995-03-15'";

const Q06: &str = "\
SELECT l.l_extendedprice, l.l_discount \
FROM lineitem l \
WHERE l.l_discount > '5' AND l.l_quantity < '24'";

const Q07: &str = "\
SELECT l.l_orderkey \
FROM lineitem l \
WHERE l.l_orderkey IN (SELECT o_orderkey FROM orders)";

const Q08: &str = "\
WITH revenue AS ( \
  SELECT l_orderkey, l_extendedprice FROM lineitem WHERE l_discount < '10' \
) \
SELECT o.o_orderkey \
FROM orders o \
INNER JOIN revenue r ON o.o_orderkey = r.l_orderkey \
WHERE o.o_orderstatus = 'F'";

const Q09: &str = "\
SELECT c_custkey FROM customer WHERE c_mktsegment = 'BUILDING' \
UNION \
SELECT o_custkey FROM orders WHERE o_orderstatus = 'F'";

const Q10: &str = "\
SELECT c_custkey, ROW_NUMBER() OVER (PARTITION BY c_mktsegment ORDER BY c_custkey) AS rn \
FROM customer";

const Q12: &str = "\
SELECT CAST(l_quantity AS INT) AS qty \
FROM lineitem l \
WHERE CAST(l_quantity AS INT) > 10";

fn setup() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!(
        "noedb-tpch-lite-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let eng = LocalEngine::open_throughput(&dir).expect("open");

    for i in 0..CUSTOMERS {
        let k = i.to_string();
        let seg = if i % 5 == 0 { "BUILDING" } else { "AUTOMOBILE" };
        eng.put_row_default("customer", &k, "c_custkey", k.as_bytes())
            .unwrap();
        eng.put_row_default("customer", &k, "c_mktsegment", seg.as_bytes())
            .unwrap();
    }

    for i in 0..ORDERS {
        let k = i.to_string();
        let cust = (i % CUSTOMERS).to_string();
        let date = format!("1994-{:02}-{:02}", (i % 12) + 1, (i % 28) + 1);
        let status = if i % 3 == 0 { "F" } else { "O" };
        eng.put_row_default("orders", &k, "o_orderkey", k.as_bytes())
            .unwrap();
        eng.put_row_default("orders", &k, "o_custkey", cust.as_bytes())
            .unwrap();
        eng.put_row_default("orders", &k, "o_orderdate", date.as_bytes())
            .unwrap();
        eng.put_row_default("orders", &k, "o_orderstatus", status.as_bytes())
            .unwrap();
    }

    for i in 0..PARTS {
        let k = i.to_string();
        let ty = if i % 4 == 0 {
            "STANDARD BRASS"
        } else {
            "ECONOMY STEEL"
        };
        eng.put_row_default("part", &k, "p_partkey", k.as_bytes())
            .unwrap();
        eng.put_row_default("part", &k, "p_type", ty.as_bytes())
            .unwrap();
    }

    for i in 0..LINEITEMS {
        let k = i.to_string();
        let order = (i % ORDERS).to_string();
        let part = (i % PARTS).to_string();
        let qty = (i % 50 + 1).to_string();
        let disc = (i % 15).to_string();
        let price = format!("{}", 100 + (i % 900));
        eng.put_row_default("lineitem", &k, "l_orderkey", order.as_bytes())
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_partkey", part.as_bytes())
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_quantity", qty.as_bytes())
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_discount", disc.as_bytes())
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_extendedprice", price.as_bytes())
            .unwrap();
    }

    for sql in [
        "CREATE INDEX idx_orders_cust ON orders (o_custkey)",
        "CREATE INDEX idx_lineitem_order ON lineitem (l_orderkey)",
        "CREATE INDEX idx_lineitem_part ON lineitem (l_partkey)",
    ] {
        eng.execute(sql).expect("create index");
    }

    for table in ["customer", "orders", "lineitem", "part"] {
        eng.execute(&format!("ANALYZE TABLE {table}"))
            .expect("analyze");
    }

    // Warm-up planner cache / JIT-ish first run.
    let _ = eng.execute(Q06).expect("warmup");

    (eng, dir)
}

fn bench_queries(c: &mut Criterion) {
    let (eng, _dir) = setup();
    let mut group = c.benchmark_group("tpch_lite");
    group.throughput(Throughput::Elements(1));

    let queries = [
        ("Q03_join_filter", Q03),
        ("Q06_scan_filter", Q06),
        ("Q07_in_subquery", Q07),
        ("Q08_cte_join", Q08),
        ("Q09_union", Q09),
        ("Q10_window_rank", Q10),
        ("Q12_cast_filter", Q12),
    ];

    for (name, sql) in queries {
        group.bench_function(name, |b| {
            b.iter(|| black_box(eng.execute(sql).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_queries);
criterion_main!(benches);
