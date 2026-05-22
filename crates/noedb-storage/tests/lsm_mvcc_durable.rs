//! Durable MVCC via LSM internal keys (Phase 2 Week 7).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use noedb_storage::{LsmConfig, LsmTree, Version};

#[test]
fn put_two_versions_read_by_timestamp_after_reopen() {
    let dir = std::env::temp_dir().join(format!(
        "noedb-lsm-mvcc-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 1_000_000,
                l0_compaction_trigger: 99,
                ..LsmConfig::default()
            },
        )
        .unwrap();
        tree.put_version(b"acct\0bal", &Version::put(1, b"100".to_vec()))
            .unwrap();
        tree.put_version(b"acct\0bal", &Version::put(2, b"200".to_vec()))
            .unwrap();
        tree.sync().unwrap();
    }
    let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
    assert_eq!(
        tree.get_at_ts(b"acct\0bal", 1).unwrap(),
        Some(b"100".to_vec())
    );
    assert_eq!(
        tree.get_at_ts(b"acct\0bal", 2).unwrap(),
        Some(b"200".to_vec())
    );
    let _ = std::fs::remove_dir_all(dir);
}
