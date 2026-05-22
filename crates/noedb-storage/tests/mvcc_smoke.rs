//! MVCC integration tests (Phase 2 Week 7).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_storage::{MvccMemTable, TimestampOracle};

#[test]
fn write_v1_v2_read_by_timestamp() {
    let mut mt = MvccMemTable::new();
    mt.put(b"acct:1:balance".to_vec(), b"100".to_vec(), 1);
    mt.put(b"acct:1:balance".to_vec(), b"200".to_vec(), 2);

    assert_eq!(mt.get_at_ts(b"acct:1:balance", 1), Some(b"100".to_vec()));
    assert_eq!(mt.get_at_ts(b"acct:1:balance", 2), Some(b"200".to_vec()));
}

#[test]
fn oracle_monotonic() {
    let oracle = TimestampOracle::new();
    let a = oracle.next();
    let b = oracle.next();
    assert!(b > a);
}

#[test]
fn gc_stabilizes_versions() {
    let mut mt = MvccMemTable::new();
    for i in 1..=100u64 {
        mt.put(b"k".to_vec(), format!("v{i}").into_bytes(), i);
    }
    let before = mt.len();
    let stats = noedb_storage::gc_versions(&mut mt, 95);
    assert!(stats.versions_pruned > 0);
    assert!(mt.len() < before);
    assert_eq!(mt.get_at_ts(b"k", 100), Some(b"v100".into()));
}
