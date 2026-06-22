//! TPC-H lite smoke queries (Week 44).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use noedb_engine::LocalEngine;

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-tpch-{}-{}-{seq}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

fn seed_mini(eng: &LocalEngine) {
    for i in 0..20u32 {
        let k = i.to_string();
        let seg = if i % 5 == 0 { "BUILDING" } else { "AUTO" };
        eng.put_row_default("customer", &k, "c_custkey", k.as_bytes())
            .unwrap();
        eng.put_row_default("customer", &k, "c_mktsegment", seg.as_bytes())
            .unwrap();
    }
    for i in 0..50u32 {
        let k = i.to_string();
        eng.put_row_default("orders", &k, "o_orderkey", k.as_bytes())
            .unwrap();
        eng.put_row_default("orders", &k, "o_custkey", (i % 20).to_string().as_bytes())
            .unwrap();
        eng.put_row_default("orders", &k, "o_orderdate", b"1994-06-01")
            .unwrap();
        eng.put_row_default("orders", &k, "o_orderstatus", b"F")
            .unwrap();
    }
    for i in 0..10u32 {
        let k = i.to_string();
        let ty = if i == 0 { "STANDARD BRASS" } else { "OTHER" };
        eng.put_row_default("part", &k, "p_partkey", k.as_bytes())
            .unwrap();
        eng.put_row_default("part", &k, "p_type", ty.as_bytes())
            .unwrap();
    }
    for i in 0..80u32 {
        let k = i.to_string();
        eng.put_row_default(
            "lineitem",
            &k,
            "l_orderkey",
            (i % 50).to_string().as_bytes(),
        )
        .unwrap();
        eng.put_row_default("lineitem", &k, "l_partkey", (i % 10).to_string().as_bytes())
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_quantity", b"15")
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_discount", b"7")
            .unwrap();
        eng.put_row_default("lineitem", &k, "l_extendedprice", b"100")
            .unwrap();
    }
    eng.execute("CREATE INDEX idx_orders_cust ON orders (o_custkey)")
        .unwrap();
    eng.execute("ANALYZE TABLE customer").unwrap();
    eng.execute("ANALYZE TABLE orders").unwrap();
    eng.execute("ANALYZE TABLE lineitem").unwrap();
    eng.execute("ANALYZE TABLE part").unwrap();
}

#[test]
fn tpch_lite_corpus_executes() {
    let (eng, dir) = temp_engine();
    seed_mini(&eng);

    let queries = [
        (
            "Q03",
            "SELECT l.l_orderkey FROM lineitem l \
             INNER JOIN orders o ON l.l_orderkey = o.o_orderkey \
             INNER JOIN customer c ON o.o_custkey = c.c_custkey \
             WHERE c.c_mktsegment = 'BUILDING'",
        ),
        (
            "Q06",
            "SELECT l.l_extendedprice FROM lineitem l \
             WHERE l.l_discount > '5' AND l.l_quantity < '24'",
        ),
        (
            "Q07",
            "SELECT l.l_orderkey FROM lineitem l \
             WHERE l.l_orderkey IN (SELECT o_orderkey FROM orders)",
        ),
        (
            "Q08",
            "WITH rev AS (SELECT l_orderkey FROM lineitem WHERE l_discount < '10') \
             SELECT o.o_orderkey FROM orders o \
             INNER JOIN rev r ON o.o_orderkey = r.l_orderkey",
        ),
        (
            "Q09",
            "SELECT c_custkey FROM customer WHERE c_mktsegment = 'BUILDING' \
             UNION SELECT o_custkey FROM orders",
        ),
        (
            "Q10",
            "SELECT c_custkey, ROW_NUMBER() OVER \
             (PARTITION BY c_mktsegment ORDER BY c_custkey) AS rn FROM customer",
        ),
        (
            "Q12",
            "SELECT CAST(l_quantity AS INT) AS q FROM lineitem \
             WHERE CAST(l_quantity AS INT) > 10",
        ),
    ];

    for (name, sql) in queries {
        let out = eng
            .execute(sql)
            .unwrap_or_else(|e| panic!("{name} failed: {e:?}"));
        assert!(
            !out.columns.is_empty() || out.rows.is_empty(),
            "{name} returned columns"
        );
    }

    let explain = eng
        .explain(
            "SELECT l.l_orderkey FROM lineitem l \
             INNER JOIN orders o ON l.l_orderkey = o.o_orderkey \
             WHERE o.o_orderstatus = 'F'",
        )
        .unwrap();
    assert!(
        explain.contains("Join") || explain.contains("SemiJoin") || explain.contains("SeqScan"),
        "{explain}"
    );

    let _ = std::fs::remove_dir_all(dir);
}
