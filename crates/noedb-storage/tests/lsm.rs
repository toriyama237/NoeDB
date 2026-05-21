//! LSM integration tests (Weeks 11–16).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use noedb_storage::{LsmConfig, LsmTree, WalSegmentManager, WalSyncMode};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("noedb-int-{name}-{nanos}"))
}

#[test]
fn wal_segment_replay_after_simulated_crash() {
    let dir = temp_dir("crash");
    {
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 512 * 1024,
                l0_compaction_trigger: 99,
                wal_sync: WalSyncMode::EveryAppend,
            },
        )
        .unwrap();
        for i in 0..100u32 {
            let k = format!("k:{i:03}");
            tree.put(k.as_bytes(), b"v").unwrap();
        }
    }
    let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
    assert_eq!(tree.get(b"k:099").unwrap(), Some(b"v".to_vec()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ten_k_durability_test() {
    let dir = temp_dir("10k-dur");
    let mut tree = LsmTree::open(
        &dir,
        LsmConfig {
            max_mem_bytes: 2048,
            l0_compaction_trigger: 4,
            wal_sync: WalSyncMode::OnFlush,
        },
    )
    .unwrap();

    for i in 0..10_000u32 {
        let k = format!("row:{i:05}");
        tree.put(k.as_bytes(), b"payload").unwrap();
    }
    tree.sync().unwrap();

    drop(tree);
    let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
    for i in (0..10_000u32).step_by(50) {
        let k = format!("row:{i:05}");
        assert_eq!(tree.get(k.as_bytes()).unwrap(), Some(b"payload".to_vec()));
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn wal_segments_list_and_replay() {
    let dir = temp_dir("segments");
    let mut mgr = WalSegmentManager::open(&dir).unwrap();
    mgr.append(&noedb_storage::LogEntry::put(b"x".to_vec(), b"1".to_vec()))
        .unwrap();
    mgr.rotate().unwrap();
    mgr.append(&noedb_storage::LogEntry::put(b"y".to_vec(), b"2".to_vec()))
        .unwrap();
    let mut table = noedb_storage::MemTable::new();
    let n = WalSegmentManager::replay_all(&dir, &mut table).unwrap();
    assert_eq!(n, 2);
    let _ = std::fs::remove_dir_all(dir);
}
