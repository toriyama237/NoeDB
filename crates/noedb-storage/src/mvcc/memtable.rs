//! Multi-version in-memory table (BTreeMap internal keys).

use std::collections::BTreeMap;
use std::ops::Bound;

use super::key::{decode_user_key, encode_internal_key, user_key_prefix_end};
use super::read_view::ReadView;
use super::{CommitTs, TxnId, Version};

/// In-memory MVCC store: multiple versions per user key.
#[derive(Debug, Default)]
pub struct MvccMemTable {
    /// Internal key → version payload.
    data: BTreeMap<Vec<u8>, Version>,
    /// Uncommitted intents: internal key → owning txn.
    intents: BTreeMap<Vec<u8>, TxnId>,
}

#[allow(clippy::needless_pass_by_value)] // `commit_ts` is forwarded into `Version::*` constructors
impl MvccMemTable {
    /// Empty memtable.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of internal records (all versions).
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// True when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Commit a put at `commit_ts`.
    pub fn put(&mut self, user_key: Vec<u8>, value: Vec<u8>, commit_ts: CommitTs) {
        let ik = encode_internal_key(&user_key, commit_ts);
        self.intents.remove(&ik);
        let ver = Version::put(commit_ts, value);
        self.data.insert(ik, ver);
    }

    /// Commit a delete tombstone at `commit_ts`.
    pub fn delete(&mut self, user_key: Vec<u8>, commit_ts: CommitTs) {
        let ik = encode_internal_key(&user_key, commit_ts);
        self.intents.remove(&ik);
        let ver = Version::tombstone(commit_ts);
        self.data.insert(ik, ver);
    }

    /// Stage uncommitted write intent for `txn_id`.
    pub fn put_intent(&mut self, user_key: Vec<u8>, value: Vec<u8>, txn_id: TxnId) {
        let ik = encode_internal_key(&user_key, u64::MAX);
        self.intents.insert(ik.clone(), txn_id);
        let ver = Version::intent(value);
        self.data.insert(ik, ver);
    }

    /// Stage uncommitted delete intent.
    pub fn delete_intent(&mut self, user_key: Vec<u8>, txn_id: TxnId) {
        let ik = encode_internal_key(&user_key, u64::MAX);
        self.intents.insert(ik.clone(), txn_id);
        let ver = Version::delete_intent();
        self.data.insert(ik, ver);
    }

    /// Drop all intents owned by `txn_id` (rollback).
    pub fn abort_intents(&mut self, txn_id: TxnId) {
        let keys: Vec<_> = self
            .intents
            .iter()
            .filter(|(_, &t)| t == txn_id)
            .map(|(k, _)| k.clone())
            .collect();
        for ik in keys {
            self.intents.remove(&ik);
            self.data.remove(&ik);
        }
    }

    /// Latest visible value at snapshot (None = missing or deleted).
    #[must_use]
    pub fn get(&self, user_key: &[u8], view: &ReadView) -> Option<Vec<u8>> {
        self.get_version(user_key, view)
            .filter(|v| !v.deleted)
            .map(|v| v.value.clone())
    }

    /// Read at explicit `read_ts` (Week 7 — time-travel without full txn).
    #[must_use]
    pub fn get_at_ts(&self, user_key: &[u8], read_ts: CommitTs) -> Option<Vec<u8>> {
        let view = ReadView::new(0, read_ts, std::collections::BTreeSet::new());
        self.get(user_key, &view)
    }

    /// Scan all visible keys at `read_ts`.
    #[must_use]
    pub fn scan_at_ts(&self, read_ts: CommitTs) -> Vec<(Vec<u8>, Vec<u8>)> {
        let view = ReadView::new(0, read_ts, std::collections::BTreeSet::new());
        self.scan(&view).collect()
    }

    /// Latest visible version (including tombstones).
    #[must_use]
    pub fn get_version(&self, user_key: &[u8], view: &ReadView) -> Option<&Version> {
        let start = user_key.to_vec();
        let end = user_key_prefix_end(user_key);
        let mut entries: Vec<(&Vec<u8>, &Version)> = self
            .data
            .range((Bound::Included(start), Bound::Excluded(end)))
            .collect();
        entries.sort_by(|a, b| b.0.cmp(a.0));
        let mut best: Option<&Version> = None;
        for (ik, ver) in entries {
            let writer = self.intents.get(ik).copied();
            if !view.is_visible(ver, writer) {
                continue;
            }
            if best.is_none_or(|b| ver.commit_ts > b.commit_ts) {
                best = Some(ver);
            }
        }
        best
    }

    /// All versions for a user key (newest first), for tests / GC.
    #[must_use]
    pub fn versions(&self, user_key: &[u8]) -> Vec<(CommitTs, Version)> {
        let start = encode_internal_key(user_key, u64::MAX);
        let end = user_key_prefix_end(user_key);
        self.data
            .range(start..end)
            .map(|(ik, v)| {
                let ts = v.commit_ts;
                let _ = decode_user_key(ik);
                (ts, v.clone())
            })
            .collect()
    }

    /// Scan visible keys at snapshot (one row per user key).
    pub fn scan<'a>(&'a self, view: &'a ReadView) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a {
        let mut keys = std::collections::BTreeSet::new();
        for ik in self.data.keys() {
            keys.insert(decode_user_key(ik).to_vec());
        }
        keys.into_iter().filter_map(move |key| {
            let ver = self.get_version(&key, view)?;
            if ver.deleted {
                return None;
            }
            Some((key, ver.value.clone()))
        })
    }

    /// Bounded visible scan: latest non-deleted version per user key in
    /// `[start, end)`, in one pass over the internal-key range.
    ///
    /// [`MvccMemTable::scan`] walks the whole table and re-resolves each
    /// key; table scans over one prefix paid the cost of every other
    /// table's versions. Internal keys are `user_key || suffix`, so a
    /// 9-byte `0xFF` guard above `end` covers versions of user keys that
    /// are proper prefixes of `end`; the exact filter runs per entry.
    #[must_use]
    pub fn scan_range(&self, view: &ReadView, start: &[u8], end: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.latest_in_range(view, start, end)
            .into_iter()
            .filter(|(_, ver)| !ver.deleted)
            .map(|(k, ver)| (k.to_vec(), ver.value.clone()))
            .collect()
    }

    /// Key-only variant of [`MvccMemTable::scan_range`] — no value copies.
    #[must_use]
    pub fn scan_range_keys(&self, view: &ReadView, start: &[u8], end: &[u8]) -> Vec<Vec<u8>> {
        self.latest_in_range(view, start, end)
            .into_iter()
            .filter(|(_, ver)| !ver.deleted)
            .map(|(k, _)| k.to_vec())
            .collect()
    }

    /// Latest visible version per user key in `[start, end)` (borrowed).
    fn latest_in_range(
        &self,
        view: &ReadView,
        start: &[u8],
        end: &[u8],
    ) -> BTreeMap<&[u8], &Version> {
        // Guard above `end`: internal keys append an 8-byte suffix, so
        // versions of user keys that are proper prefixes of `end` can
        // sort at or above it; 9 bytes of 0xFF dominate any suffix.
        let mut guard = end.to_vec();
        guard.extend_from_slice(&[0xFF; 9]);
        let upper = if end.is_empty() {
            Bound::Unbounded
        } else {
            Bound::Excluded(guard.as_slice())
        };
        let iter = self.data.range::<[u8], _>((Bound::Included(start), upper));

        let mut latest: BTreeMap<&[u8], &Version> = BTreeMap::new();
        for (ik, ver) in iter {
            let user = decode_user_key(ik);
            if user < start || (!end.is_empty() && user >= end) {
                continue;
            }
            let writer = self.intents.get(ik).copied();
            if !view.is_visible(ver, writer) {
                continue;
            }
            let newer = latest
                .get(user)
                .is_none_or(|prev| ver.commit_ts > prev.commit_ts);
            if newer {
                latest.insert(user, ver);
            }
        }
        latest
    }

    /// Remove internal keys with `commit_ts < min_ts` (GC).
    pub fn prune_below(&mut self, min_ts: CommitTs) -> usize {
        let before = self.data.len();
        self.data.retain(|ik, ver| {
            if ver.commit_ts == 0 {
                return true;
            }
            if ver.commit_ts >= min_ts {
                return true;
            }
            self.intents.remove(ik);
            false
        });
        before - self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_v1_then_v2_read_both_by_timestamp() {
        let mut mt = MvccMemTable::new();
        mt.put(b"k".to_vec(), b"v1".to_vec(), 1);
        mt.put(b"k".to_vec(), b"v2".to_vec(), 2);

        assert_eq!(mt.get_at_ts(b"k", 1), Some(b"v1".to_vec()));
        assert_eq!(mt.get_at_ts(b"k", 2), Some(b"v2".to_vec()));
        assert_eq!(mt.get_at_ts(b"k", 0), None);
    }

    #[test]
    fn delete_tombstone_visible() {
        let mut mt = MvccMemTable::new();
        mt.put(b"k".to_vec(), b"v".to_vec(), 1);
        mt.delete(b"k".to_vec(), 2);
        assert_eq!(mt.get_at_ts(b"k", 1), Some(b"v".to_vec()));
        assert_eq!(mt.get_at_ts(b"k", 2), None);
    }
}
