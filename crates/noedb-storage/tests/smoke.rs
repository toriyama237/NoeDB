//! Integration smoke tests for `noedb-storage`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use noedb_storage::{MemTable, StorageError};

#[test]
fn memtable_implements_storage_engine_trait() {
    let mut engine: MemTable = MemTable::new();
    engine.put(b"alpha", b"1").unwrap();
    engine.put(b"beta", b"2").unwrap();
    assert_eq!(engine.get(b"alpha").unwrap(), Some(b"1".to_vec()));
    assert_eq!(engine.get(b"missing").unwrap(), None);
    assert!(engine.delete(b"alpha").unwrap());
    assert_eq!(engine.get(b"alpha").unwrap(), None);

    let keys: Vec<_> = engine.iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![b"beta".to_vec()]);
}

#[test]
fn thousand_entry_durability_in_memory() {
    let mut mt = MemTable::new();
    for i in 0..1_000u32 {
        let key = format!("row:{i:05}");
        mt.put(key.as_bytes(), b"payload").unwrap();
    }
    assert_eq!(mt.len(), 1_000);
    for i in 0..1_000u32 {
        let key = format!("row:{i:05}");
        assert_eq!(mt.get(key.as_bytes()).unwrap(), Some(b"payload".to_vec()));
    }
}

#[test]
fn empty_key_surfaces_structured_error() {
    let mut mt = MemTable::new();
    let err = mt.put(b"", b"x").unwrap_err();
    assert_eq!(err, StorageError::invalid_input("key must not be empty"));
}
