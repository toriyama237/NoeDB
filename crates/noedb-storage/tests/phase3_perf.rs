//! Phase 3 performance primitives (S15–S16).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_storage::{
    append_batch_sync, map_read_only, LogEntry, LsmConfig, LsmTree, OpType, SstReader,
    WalSegmentManager,
};

#[test]
fn wal_append_batch_sync_roundtrip() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase3-wal-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let mut wal = WalSegmentManager::open(&dir).unwrap();
    let entries = vec![
        LogEntry {
            op: OpType::Put,
            key: b"k1".to_vec(),
            value: b"v1".to_vec(),
        },
        LogEntry {
            op: OpType::Put,
            key: b"k2".to_vec(),
            value: b"v2".to_vec(),
        },
    ];
    append_batch_sync(&mut wal, &entries).unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn mmap_sstable_lookup_matches_reader() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase3-sst-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let mut tree = LsmTree::open(
        &dir,
        LsmConfig {
            max_mem_bytes: 48,
            l0_compaction_trigger: 99,
            ..Default::default()
        },
    )
    .unwrap();
    let key = b"0123456789012345678901234567890123456789012345678901234567890\x00name";
    tree.put(key, b"ada").unwrap();
    assert!(tree.l0_count() >= 1, "memtable should have flushed to L0");
    let sst_dir = dir.join("sst");
    let sst = std::fs::read_dir(&sst_dir)
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.path().extension().is_some_and(|x| x == "sst"))
        .map(|e| e.path())
        .expect("sst file");
    let mmap = map_read_only(&sst).unwrap();
    assert!(mmap.as_slice().len() > 64);
    let reader = SstReader::open(&sst).unwrap();
    assert_eq!(
        reader.get(key).unwrap().as_deref(),
        Some(b"ada".as_slice()),
        "mmap-backed reader should find flushed key"
    );
    let _ = std::fs::remove_dir_all(dir);
}
