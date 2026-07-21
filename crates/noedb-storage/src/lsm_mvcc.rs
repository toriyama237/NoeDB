//! MVCC read/write helpers for [`LsmTree`](crate::lsm::LsmTree).

use crate::error::StorageError;
use crate::lsm::LsmTree;
use crate::mvcc::{
    decode_or_legacy, decode_user_key, encode_internal_key, encode_version, user_key_prefix_end,
    CommitTs, ReadView, Version,
};
use crate::wal::LogEntry;

impl LsmTree {
    /// Persist one MVCC version under internal key `user_key || !commit_ts`.
    pub fn put_version(&mut self, user_key: &[u8], version: &Version) -> Result<(), StorageError> {
        let ik = encode_internal_key(user_key, version.commit_ts);
        let val = encode_version(version)?;
        self.wal.append(&LogEntry::put(ik.clone(), val.clone()))?;
        self.wal_pending += 1;
        self.maybe_sync_wal()?;
        self.active.put(&ik, &val)?;
        self.maybe_flush_and_compact()?;
        Ok(())
    }

    /// Latest committed cell value for `user_key` (newest visible version).
    pub fn get_latest(&self, user_key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        let view = ReadView::new(0, u64::MAX, std::collections::BTreeSet::new());
        self.get_visible(user_key, &view)
    }

    /// Snapshot read at `read_ts`.
    pub fn get_at_ts(
        &self,
        user_key: &[u8],
        read_ts: CommitTs,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let view = ReadView::new(0, read_ts, std::collections::BTreeSet::new());
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
        use std::collections::HashMap;
        // Determine the newest commit_ts per user key first. We must NEVER
        // collect a key's latest version (it is the live value); only
        // superseded older versions below the retain watermark are eligible.
        // Dropping the latest version unconditionally would silently delete
        // rows that simply have not been updated recently.
        let mut latest: HashMap<Vec<u8>, CommitTs> = HashMap::new();
        for (ik, raw) in self.active.iter() {
            if let Ok(v) = decode_or_legacy(&raw) {
                let uk = decode_user_key(&ik).to_vec();
                let slot = latest.entry(uk).or_insert(0);
                if v.commit_ts > *slot {
                    *slot = v.commit_ts;
                }
            }
        }
        let keys: Vec<Vec<u8>> = self
            .active
            .iter()
            .filter_map(|(ik, raw)| {
                let v = decode_or_legacy(&raw).ok()?;
                let uk = decode_user_key(&ik);
                let is_latest = latest.get(uk).is_some_and(|newest| *newest == v.commit_ts);
                if v.commit_ts > 0 && v.commit_ts < min_retain_ts && !is_latest {
                    Some(ik.clone())
                } else {
                    None
                }
            })
            .collect();
        let n = keys.len();
        for ik in keys {
            let _ = self.active.delete(&ik);
        }
        n
    }

    pub(crate) fn find_visible_version(
        &self,
        user_key: &[u8],
        view: &ReadView,
    ) -> Result<Option<Version>, StorageError> {
        let start = user_key.to_vec();
        let end = user_key_prefix_end(user_key);
        let mut best: Option<Version> = None;

        let mut consider = |ver: Version| {
            if !view.is_visible(&ver, None) {
                return;
            }
            let better = match &best {
                None => true,
                Some(b) => ver.commit_ts > b.commit_ts,
            };
            if better {
                best = Some(ver);
            }
        };

        // The scan window `[user_key, prefix_end)` also covers longer keys
        // sharing the prefix (e.g. legacy cell keys `key\0col`): only exact
        // matches on the decoded user key are versions of this key.
        for (ik, raw) in self.active.range(&start, &end) {
            if decode_user_key(&ik) != user_key && ik.as_slice() != user_key {
                continue;
            }
            consider(decode_or_legacy(&raw)?);
        }

        // Bounded block-index seek: only blocks that may hold versions of
        // this user key are read (was a full SST scan per point read).
        for path in self.level0.iter().rev().chain(self.level1.iter().rev()) {
            let Ok(reader) = self.cached_reader(path) else {
                continue;
            };
            let Ok(iter) = reader.scan_range(&start, &end) else {
                continue;
            };
            for (ik, raw) in iter.flatten() {
                if decode_user_key(&ik) != user_key && ik.as_slice() != user_key {
                    continue;
                }
                consider(decode_or_legacy(&raw)?);
            }
        }

        Ok(best)
    }
}
