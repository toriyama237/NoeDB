//! MVCC read/write helpers for [`LsmTree`](crate::lsm::LsmTree).

use crate::error::StorageError;
use crate::lsm::LsmTree;
use crate::mvcc::{
    decode_or_legacy, decode_user_key, encode_internal_key, encode_version,
    user_key_prefix_end, CommitTs, ReadView, Version,
};
use crate::wal::LogEntry;

impl LsmTree {
    /// Persist one MVCC version under internal key `user_key || !commit_ts`.
    pub fn put_version(&mut self, user_key: &[u8], version: &Version) -> Result<(), StorageError> {
        let ik = encode_internal_key(user_key, version.commit_ts);
        let val = encode_version(version)?;
        self.wal.append(&LogEntry::put(ik.clone(), val.clone()))?;
        self.active.put(&ik, &val)?;
        self.maybe_flush_and_compact()?;
        Ok(())
    }

    /// Latest committed cell value for `user_key` (newest visible version).
    pub fn get_latest(&self, user_key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        let view = ReadView::new(0, u64::MAX, Default::default());
        self.get_visible(user_key, &view)
    }

    /// Snapshot read at `read_ts`.
    pub fn get_at_ts(&self, user_key: &[u8], read_ts: CommitTs) -> Result<Option<Vec<u8>>, StorageError> {
        let view = ReadView::new(0, read_ts, Default::default());
        self.get_visible(user_key, &view)
    }

    /// Visible cell bytes at snapshot (None if missing or tombstoned).
    pub fn get_visible(
        &self,
        user_key: &[u8],
        view: &ReadView,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(ver) = self.find_visible_version(user_key, view)? {
            if ver.deleted {
                return Ok(None);
            }
            return Ok(Some(ver.value));
        }
        Ok(None)
    }

    /// Prune internal keys with `commit_ts < min_retain_ts` from the active memtable.
    pub fn gc_mvcc_active(&mut self, min_retain_ts: CommitTs) -> usize {
        let keys: Vec<Vec<u8>> = self
            .active
            .iter()
            .filter_map(|(ik, raw)| {
                decode_or_legacy(&raw).ok().and_then(|v| {
                    if v.commit_ts > 0 && v.commit_ts < min_retain_ts {
                        Some(ik.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();
        let n = keys.len();
        for ik in keys {
            let _ = self.active.delete(&ik);
        }
        n
    }

    fn find_visible_version(
        &self,
        user_key: &[u8],
        view: &ReadView,
    ) -> Result<Option<Version>, StorageError> {
        let start = user_key.to_vec();
        let end = user_key_prefix_end(user_key);
        let mut best: Option<Version> = None;

        let mut active_entries: Vec<_> = self.active.range(&start, &end).collect();
        active_entries.sort_by(|a, b| b.0.cmp(&a.0));
        for (ik, raw) in active_entries {
            let ver = decode_or_legacy(&raw)?;
            if !view.is_visible(&ver, None) {
                continue;
            }
            if best.as_ref().is_none_or(|b| ver.commit_ts > b.commit_ts) {
                let _ = decode_user_key(&ik);
                best = Some(ver);
            }
        }

        for path in self.level0.iter().rev().chain(self.level1.iter().rev()) {
            if let Ok(reader) = crate::sstable::SstReader::open(path) {
                if let Ok(iter) = reader.scan() {
                    for item in iter.flatten() {
                        let (ik, raw) = item;
                        if ik.as_slice() < start.as_slice() || ik.as_slice() >= end.as_slice() {
                            continue;
                        }
                        let ver = decode_or_legacy(&raw)?;
                        if !view.is_visible(&ver, None) {
                            continue;
                        }
                        if best.as_ref().is_none_or(|b| ver.commit_ts > b.commit_ts) {
                            best = Some(ver);
                        }
                    }
                }
            }
        }

        Ok(best)
    }
}
