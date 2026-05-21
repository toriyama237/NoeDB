//! Durable store integration tests (Week 10).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use noedb_storage::{DurableStore, Wal};

fn temp_wal(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("noedb-{name}-{nanos}.log"))
}

#[test]
fn durable_store_survives_reopen_via_wal() {
    let path = temp_wal("reopen");
    {
        let mut store = DurableStore::open(&path, 64 * 1024).unwrap();
        store.put(b"session", b"alive").unwrap();
    }

    let entries = Wal::read_all(&path).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key, b"session");
    assert_eq!(entries[0].value, b"alive");

    let _ = std::fs::remove_file(path);
}

#[test]
fn thousand_puts_with_small_rotation_threshold() {
    let path = temp_wal("1k");
    let mut store = DurableStore::open(&path, 256).unwrap();

    for i in 0..1_000u32 {
        let key = format!("k:{i:04}");
        let val = format!("v:{i}");
        store.put(key.as_bytes(), val.as_bytes()).unwrap();
    }

    assert!(store.immutable_count() >= 1);
    for i in 0..1_000u32 {
        let key = format!("k:{i:04}");
        let val = format!("v:{i}");
        assert_eq!(store.get(key.as_bytes()).unwrap(), Some(val.into_bytes()));
    }

    let wal_entries = Wal::read_all(&path).unwrap();
    assert_eq!(wal_entries.len(), 1_000);

    let _ = std::fs::remove_file(path);
}
