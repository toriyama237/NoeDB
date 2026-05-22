//! Transaction manager tests (Phase 2 Weeks 8–11).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::thread;

use noedb_txn::{TxnError, TxnManager};

#[test]
fn begin_commit_read_your_writes() {
    let mgr = TxnManager::new();
    mgr.begin(1).unwrap();
    mgr.with_txn(1, |txn| {
        txn.put(b"k".to_vec(), b"v".to_vec());
    })
    .unwrap();
    assert_eq!(mgr.get(1, b"k").unwrap(), Some(b"v".to_vec()));
    mgr.commit(1).unwrap();
    assert_eq!(mgr.get_at_ts(b"k", 1), None);
    assert!(mgr.get_at_ts(b"k", 2).is_some());
}

#[test]
fn rollback_discards_writes() {
    let mgr = TxnManager::new();
    mgr.begin(1).unwrap();
    mgr.with_txn(1, |txn| txn.put(b"k".to_vec(), b"v".to_vec()))
        .unwrap();
    mgr.rollback(1).unwrap();
    assert!(matches!(mgr.get(1, b"k"), Err(TxnError::NoActiveTxn)));
}

#[test]
fn hundred_concurrent_txns_no_cross_talk() {
    let mgr = Arc::new(TxnManager::new());
    let mut handles = Vec::new();
    for session in 0..100u64 {
        let m = Arc::clone(&mgr);
        handles.push(thread::spawn(move || {
            m.begin(session).unwrap();
            let key = format!("k:{session}").into_bytes();
            m.with_txn(session, |txn| txn.put(key.clone(), b"ok".to_vec()))
                .unwrap();
            assert_eq!(m.get(session, &key).unwrap(), Some(b"ok".to_vec()));
            m.commit(session).unwrap();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(mgr.active_count(), 0);
}

#[test]
fn bank_transfer_commit_ts_ordering() {
    let mgr = TxnManager::new();
    mgr.mvcc().put(b"a".to_vec(), b"1000".to_vec(), 1);
    mgr.mvcc().put(b"b".to_vec(), b"0".to_vec(), 1);

    mgr.begin(1).unwrap();
    mgr.with_txn(1, |txn| {
        txn.put(b"a".to_vec(), b"900".to_vec());
        txn.put(b"b".to_vec(), b"100".to_vec());
    })
    .unwrap();
    let r = mgr.commit(1).unwrap();
    assert!(r.commit_ts >= 2);
    assert_eq!(mgr.get_at_ts(b"a", r.commit_ts), Some(b"900".to_vec()));
    assert_eq!(mgr.get_at_ts(b"b", r.commit_ts), Some(b"100".to_vec()));
}

#[test]
fn duplicate_begin_rejected() {
    let mgr = TxnManager::new();
    mgr.begin(1).unwrap();
    assert!(matches!(mgr.begin(1), Err(TxnError::TxnAlreadyActive)));
    mgr.rollback(1).unwrap();
}
