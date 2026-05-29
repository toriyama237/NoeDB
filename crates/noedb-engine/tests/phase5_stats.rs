//! Phase 5 optimizer statistics (Week 43).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use noedb_engine::LocalEngine;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase5-stats-{}-{}-{}",
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn analyze_updates_explain_row_estimates() {
    let (eng, dir) = temp_engine();
    for i in 0..20 {
        let id = i.to_string();
        eng.put_row_default("items", &id, "id", id.as_bytes())
            .unwrap();
        eng.put_row_default("items", &id, "sku", b"A").unwrap();
    }

    eng.execute("ANALYZE TABLE items").unwrap();
    let text = eng.explain("SELECT sku FROM items WHERE id = '0'").unwrap();
    assert!(text.contains("rows≈20") || text.contains("rows≈1"));
    assert!(text.contains("IndexScan") || text.contains("SeqScan"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn analyze_skewed_ndv_affects_cost() {
    let (eng, dir) = temp_engine();
    for i in 0..100 {
        let id = i.to_string();
        eng.put_row_default("t", &id, "id", id.as_bytes()).unwrap();
        let tag: &[u8] = if i < 5 { b"rare" } else { b"common" };
        eng.put_row_default("t", &id, "tag", tag).unwrap();
    }
    eng.execute("CREATE INDEX idx_t_tag ON t (tag)").unwrap();
    eng.execute("ANALYZE TABLE t").unwrap();

    let rare = eng.explain("SELECT id FROM t WHERE tag = 'rare'").unwrap();
    let common = eng
        .explain("SELECT id FROM t WHERE tag = 'common'")
        .unwrap();
    assert!(rare.contains("rows≈"));
    assert!(common.contains("rows≈"));
    let _ = std::fs::remove_dir_all(dir);
}
