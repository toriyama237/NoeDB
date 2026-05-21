//! MVCC version garbage collection (Phase 2 Week 13).

use super::memtable::MvccMemTable;
use super::CommitTs;

/// GC statistics (Prometheus-friendly).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GcStats {
    /// Versions pruned by GC.
    pub versions_pruned: usize,
    /// Oldest retained commit timestamp.
    pub min_retain_ts: CommitTs,
}

/// Prune versions older than `min_retain_ts` (not visible to any active snapshot).
pub fn gc_versions(table: &mut MvccMemTable, min_retain_ts: CommitTs) -> GcStats {
    let pruned = table.prune_below(min_retain_ts);
    GcStats {
        versions_pruned: pruned,
        min_retain_ts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gc_drops_old_versions_keeps_recent() {
        let mut mt = MvccMemTable::new();
        mt.put(b"k".to_vec(), b"v1".to_vec(), 1);
        mt.put(b"k".to_vec(), b"v2".to_vec(), 2);
        mt.put(b"k".to_vec(), b"v3".to_vec(), 10);

        let stats = gc_versions(&mut mt, 5);
        assert!(stats.versions_pruned >= 1);
        assert_eq!(mt.get_at_ts(b"k", 10), Some(b"v3".to_vec()));
        assert_eq!(mt.get_at_ts(b"k", 2), None);
    }
}
