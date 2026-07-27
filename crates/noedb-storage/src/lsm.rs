//! Full LSM-tree engine orchestrating MemTable, WAL, SSTables (Week 16).

use parking_lot::Mutex;
use std::collections::{hash_map::Entry, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use crate::compaction::{self, L0_COMPACTION_TRIGGER};
use crate::engine::StorageEngine;
use crate::error::StorageError;
use crate::manifest::ManifestSnapshot;
use crate::memtable::{MemTable, DEFAULT_MAX_MEM_BYTES};
use crate::sstable::{SstReader, SstWriteOptions, SstWriter};
use crate::wal::{LogEntry, WalSegmentManager, WalSyncMode};
use crate::write_stall::{WriteStallConfig, WriteStallController};

/// Configuration for [`LsmTree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LsmConfig {
    /// Rotate MemTable when active table exceeds this many bytes.
    pub max_mem_bytes: usize,
    /// Start L0→L1 compaction at this many L0 files.
    pub l0_compaction_trigger: usize,
    /// When to `fsync` the WAL (`EveryAppend` = durable per put, `OnFlush` = group commit).
    pub wal_sync: WalSyncMode,
    /// SST write profile (LZ4 + XOR by default).
    pub sst: SstWriteOptions,
    /// Group-commit: max WAL records before implicit sync on flush path.
    pub wal_batch_size: usize,
    /// Write-stall thresholds when L0 / memtable pressure rises.
    pub write_stall: WriteStallConfig,
}

impl Default for LsmConfig {
    fn default() -> Self {
        Self {
            max_mem_bytes: DEFAULT_MAX_MEM_BYTES,
            l0_compaction_trigger: L0_COMPACTION_TRIGGER,
            wal_sync: WalSyncMode::EveryAppend,
            sst: SstWriteOptions::default(),
            wal_batch_size: 1,
            write_stall: WriteStallConfig::default(),
        }
    }
}

impl LsmConfig {
    /// High-throughput profile: group-commit WAL, large memtable, rare compaction.
    #[must_use]
    pub const fn throughput() -> Self {
        Self {
            max_mem_bytes: 16 * 1024 * 1024,
            l0_compaction_trigger: 64,
            wal_sync: WalSyncMode::OnFlush,
            sst: SstWriteOptions::phase3_default(),
            wal_batch_size: 256,
            write_stall: WriteStallConfig::relaxed(),
        }
    }
}

/// Complete LSM storage engine: WAL + MemTable + SSTables + compaction.
pub struct LsmTree {
    dir: PathBuf,
    pub(crate) wal: WalSegmentManager,
    pub(crate) active: MemTable,
    pub(crate) level0: Vec<PathBuf>,
    pub(crate) level1: Vec<PathBuf>,
    config: LsmConfig,
    flushed_wal_segment: u64,
    sst_cache: Mutex<HashMap<PathBuf, SstReader>>,
    pub(crate) wal_pending: usize,
    write_stall: WriteStallController,
    manifest_seq: u64,
}

impl LsmTree {
    /// Open or create an LSM tree at `data_dir`, replaying WAL on startup.
    pub fn open(data_dir: impl AsRef<Path>, config: LsmConfig) -> Result<Self, StorageError> {
        let dir = data_dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        fs::create_dir_all(dir.join("sst"))?;

        let mut active = MemTable::new();
        WalSegmentManager::replay_all(&dir, &mut active)?;

        let manifest = ManifestSnapshot::load(&dir)?;
        let level0 = if let Some(ref m) = manifest {
            m.level0.clone()
        } else {
            compaction::list_sst_level(&dir.join("sst"), 0)?
        };
        let level1 = if let Some(ref m) = manifest {
            m.level1.clone()
        } else {
            compaction::list_sst_level(&dir.join("sst"), 1)?
        };
        let manifest_seq = manifest.map_or(0, |m| m.sequence);
        let wal = WalSegmentManager::open_with_sync(&dir, config.wal_sync)?;

        Ok(Self {
            dir,
            wal,
            active,
            level0,
            level1,
            config,
            flushed_wal_segment: 0,
            sst_cache: Mutex::new(HashMap::new()),
            wal_pending: 0,
            write_stall: WriteStallController::new(config.write_stall),
            manifest_seq,
        })
    }

    /// Open with default [`LsmConfig`].
    pub fn open_default(data_dir: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open(data_dir, LsmConfig::default())
    }

    /// Data directory path.
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    /// Durably flush buffered WAL records (no-op when already synced per append).
    pub fn sync(&mut self) -> Result<(), StorageError> {
        self.wal.sync()?;
        self.wal_pending = 0;
        Ok(())
    }

    pub(crate) fn maybe_sync_wal(&mut self) -> Result<(), StorageError> {
        if self.config.wal_sync == WalSyncMode::EveryAppend
            || self.wal_pending >= self.config.wal_batch_size
        {
            self.sync()?;
        }
        Ok(())
    }

    /// Insert or overwrite a key (WAL → MemTable → maybe flush).
    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), StorageError> {
        self.gate_write()?;
        self.wal
            .append(&LogEntry::put(key.to_vec(), value.to_vec()))?;
        self.wal_pending += 1;
        self.maybe_sync_wal()?;
        self.active.put(key, value)?;
        self.maybe_flush_and_compact()?;
        Ok(())
    }

    /// Bulk insert with a single WAL sync at the end (ideal for ingest workloads).
    pub fn put_batch(&mut self, entries: &[(&[u8], &[u8])]) -> Result<(), StorageError> {
        self.gate_write()?;
        for (key, value) in entries {
            self.wal
                .append(&LogEntry::put(key.to_vec(), value.to_vec()))?;
            self.active.put(key, value)?;
        }
        self.wal.sync()?;
        self.maybe_flush_and_compact()?;
        Ok(())
    }

    /// Delete a key durably, even when copies were already flushed to SSTs.
    ///
    /// Removes the key from the active memtable, then — if an older copy is
    /// still visible in SSTs or as an MVCC version — writes a tombstone
    /// version that shadows it (WAL-logged, crash-safe). Returns `true` when
    /// a live value existed.
    pub fn delete(&mut self, key: &[u8]) -> Result<bool, StorageError> {
        self.gate_write()?;
        self.wal.append(&LogEntry::delete(key.to_vec()))?;
        self.wal_pending += 1;
        self.maybe_sync_wal()?;
        let removed_active = self.active.delete(key)?;

        // A memtable removal cannot reach copies already flushed to disk:
        // without a tombstone the key would resurrect on the next read.
        if self.get(key)?.is_some() {
            let view = crate::mvcc::ReadView::new(0, u64::MAX, std::collections::BTreeSet::new());
            let next_ts = self
                .find_visible_version(key, &view)?
                .map_or(2, |v| v.commit_ts.saturating_add(1));
            self.put_version(key, &crate::mvcc::Version::tombstone(next_ts))?;
            self.maybe_flush_and_compact()?;
            return Ok(true);
        }

        self.maybe_flush_and_compact()?;
        Ok(removed_active)
    }

    /// Read path: active MemTable (newest) → MVCC versions → L0 → L1.
    ///
    /// Resolution order matters: the active memtable holds the freshest
    /// writes; MVCC versions (including tombstones) shadow raw SST copies;
    /// raw SSTs are consulted newest-first only when no version exists.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        if let Some(raw) = self.active.get(key)? {
            if raw.starts_with(b"MVCC") {
                if let Ok(ver) = crate::mvcc::decode_or_legacy(&raw) {
                    if ver.deleted {
                        return Ok(None);
                    }
                    return Ok(Some(ver.value));
                }
            } else {
                return Ok(Some(raw));
            }
        }

        let view = crate::mvcc::ReadView::new(0, u64::MAX, std::collections::BTreeSet::new());
        if let Some(ver) = self.find_visible_version(key, &view)? {
            if ver.deleted {
                return Ok(None);
            }
            return Ok(Some(ver.value));
        }

        for path in self.level0.iter().rev() {
            if let Some(v) = self.get_from_sst(path, key)? {
                return Ok(Some(v));
            }
        }
        for path in self.level1.iter().rev() {
            if let Some(v) = self.get_from_sst(path, key)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Number of L0 SSTable files.
    #[must_use]
    pub fn l0_count(&self) -> usize {
        self.level0.len()
    }

    /// Active MemTable byte pressure (for memory budget / OOM guards).
    #[must_use]
    pub fn memtable_pressure_bytes(&self) -> usize {
        self.active.approx_bytes()
    }

    fn get_from_sst(&self, path: &Path, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError> {
        let reader = {
            let mut cache = self.sst_cache.lock();
            match cache.entry(path.to_path_buf()) {
                Entry::Vacant(slot) => slot.insert(SstReader::open(path)?).clone(),
                Entry::Occupied(slot) => slot.get().clone(),
            }
        };
        reader.get(key)
    }

    fn clear_sst_cache(&self) {
        self.sst_cache.lock().clear();
    }

    fn gate_write(&self) -> Result<(), StorageError> {
        self.write_stall.gate_write(
            self.level0.len(),
            self.active.approx_bytes(),
            self.config.max_mem_bytes,
        )
    }

    fn persist_manifest(&mut self) -> Result<(), StorageError> {
        self.manifest_seq += 1;
        ManifestSnapshot {
            level0: self.level0.clone(),
            level1: self.level1.clone(),
            sequence: self.manifest_seq,
        }
        .commit(&self.dir)
    }

    pub(crate) fn maybe_flush_and_compact(&mut self) -> Result<(), StorageError> {
        if self.active.approx_bytes() < self.config.max_mem_bytes {
            return Ok(());
        }
        self.flush_active()?;
        if self.level0.len() >= self.config.l0_compaction_trigger {
            self.run_compaction()?;
        }
        Ok(())
    }

    fn flush_active(&mut self) -> Result<(), StorageError> {
        let frozen = core::mem::take(&mut self.active);
        if frozen.is_empty() {
            return Ok(());
        }

        let path = compaction::alloc_sst_path(&self.dir.join("sst"), 0)?;
        SstWriter::write_from_memtable(&path, &frozen)?;
        self.level0.push(path);
        self.clear_sst_cache();

        let old_wal = self.wal.rotate()?;
        for seg in self.flushed_wal_segment..=old_wal {
            self.wal.delete_segment(seg)?;
        }
        self.flushed_wal_segment = self.wal.active_id();
        self.wal_pending = 0;
        self.persist_manifest()?;
        Ok(())
    }

    fn run_compaction(&mut self) -> Result<(), StorageError> {
        let l0 = self.level0.clone();
        let l1_path = compaction::compact_level0_to_l1(&self.dir, &l0)?;
        self.level0.clear();
        self.level1.push(l1_path);
        self.clear_sst_cache();
        self.persist_manifest()?;
        Ok(())
    }

    pub(crate) fn cached_reader(&self, path: &Path) -> Result<SstReader, StorageError> {
        let mut cache = self.sst_cache.lock();
        Ok(match cache.entry(path.to_path_buf()) {
            Entry::Vacant(slot) => slot.insert(SstReader::open(path)?).clone(),
            Entry::Occupied(slot) => slot.get().clone(),
        })
    }

    /// Bounded snapshot scan: latest version per user key in `[start, end)`
    /// visible at `view`, in one merged pass over SSTs + memtable.
    pub fn range_visible(
        &self,
        start: &[u8],
        end: &[u8],
        view: &crate::mvcc::ReadView,
    ) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> {
        self.merge_visible_at(view, Some((start, end)))
            .into_iter()
            .filter_map(|(k, ver)| {
                if ver.deleted {
                    None
                } else {
                    Some((k, ver.value))
                }
            })
    }

    fn merge_visible(
        &self,
        bounds: Option<(&[u8], &[u8])>,
    ) -> Vec<(Vec<u8>, crate::mvcc::Version)> {
        let view = crate::mvcc::ReadView::new(0, u64::MAX, std::collections::BTreeSet::new());
        self.merge_visible_at(&view, bounds)
    }

    /// Merge SSTs + active memtable into latest version per user key visible
    /// at `view`, ascending by user key.
    ///
    /// With `bounds = Some((start, end))`, only internal keys near `[start,
    /// end)` are read (block-index seek per SST) and only user keys in
    /// `[start, end)` are retained — the fast path for table-prefix scans.
    fn merge_visible_at(
        &self,
        view: &crate::mvcc::ReadView,
        bounds: Option<(&[u8], &[u8])>,
    ) -> Vec<(Vec<u8>, crate::mvcc::Version)> {
        self.merge_visible_fold(view, bounds, &|value: &[u8]| value.to_vec())
            .into_iter()
            .map(|(k, (commit_ts, deleted, value))| {
                (
                    k,
                    crate::mvcc::Version {
                        commit_ts,
                        value,
                        deleted,
                    },
                )
            })
            .collect()
    }

    /// Visible **user keys** (latest version not deleted) in `[start, end)`.
    ///
    /// Same MVCC resolution as [`LsmTree::range_visible`] but never copies
    /// a value — `COUNT(*)`-style scans go from O(table bytes) to
    /// O(key bytes).
    pub fn range_visible_keys(
        &self,
        start: &[u8],
        end: &[u8],
        view: &crate::mvcc::ReadView,
    ) -> impl Iterator<Item = Vec<u8>> {
        self.merge_visible_fold(view, Some((start, end)), &|_| ())
            .into_iter()
            .filter_map(|(k, (_, deleted, ()))| (!deleted).then_some(k))
    }

    /// Core MVCC merge: newest visible version per user key, with the
    /// winner's payload produced by `wrap` (identity copy for value
    /// scans, `()` for key-only scans).
    ///
    /// Implementation: every visible candidate is appended to a flat
    /// vector, then one `sort_unstable` + linear dedup keeps the newest
    /// version per user key. Batched sorting is several times faster
    /// than the per-entry `BTreeMap` probes this used to do (which
    /// dominated `COUNT(*)`-style scans), and the output stays in
    /// ascending user-key order.
    fn merge_visible_fold<W>(
        &self,
        view: &crate::mvcc::ReadView,
        bounds: Option<(&[u8], &[u8])>,
        wrap: &impl Fn(&[u8]) -> W,
    ) -> Vec<(Vec<u8>, (u64, bool, W))> {
        let mut candidates: Vec<(Vec<u8>, u64, bool, W)> = Vec::new();
        let windows = bounds.map(|(start, end)| scan_windows(start, end));

        let mut ingest = |ik: &[u8], raw: &[u8]| {
            let is_mvcc = raw.starts_with(b"MVCC") && ik.len() > 8;
            let (commit_ts, deleted, value): (u64, bool, &[u8]) = if is_mvcc {
                match crate::mvcc::decode_version_ref(raw) {
                    Some(t) => t,
                    None => return,
                }
            } else {
                (1, false, raw)
            };
            let user: &[u8] = if is_mvcc {
                crate::mvcc::decode_user_key(ik)
            } else {
                ik
            };
            if let Some((start, end)) = bounds {
                if user < start || (!end.is_empty() && user >= end) {
                    return;
                }
            }
            // Inline of `ReadView::is_visible(ver, None)`.
            if commit_ts == 0 || commit_ts > view.snapshot_ts {
                return;
            }
            candidates.push((user.to_vec(), commit_ts, deleted, wrap(value)));
        };

        for path in self.level1.iter().chain(self.level0.iter()) {
            let Ok(reader) = self.cached_reader(path) else {
                continue;
            };
            if let Some(windows) = &windows {
                for (lo, hi) in windows {
                    if let Ok(scan) = reader.scan_range(lo, hi) {
                        for item in scan.flatten() {
                            ingest(&item.0, &item.1);
                        }
                    }
                }
            } else if let Ok(scan) = reader.scan() {
                for item in scan.flatten() {
                    ingest(&item.0, &item.1);
                }
            }
        }
        if let Some(windows) = &windows {
            for (lo, hi) in windows {
                for (k, v) in self.active.range_borrowed(lo, hi) {
                    ingest(k, v);
                }
            }
        } else {
            for (k, v) in self.active.iter_borrowed() {
                ingest(k, v);
            }
        }

        // Key ascending, then newest version first: dedup keeps index 0
        // of each user-key run. Overlapping scan windows can also feed
        // the same internal entry twice; dedup collapses those too.
        candidates.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut out: Vec<(Vec<u8>, (u64, bool, W))> = Vec::with_capacity(candidates.len());
        for (user, ts, deleted, value) in candidates {
            if out.last().is_none_or(|(prev, _)| prev != &user) {
                out.push((user, (ts, deleted, value)));
            }
        }
        out
    }
}

/// Internal-key scan windows covering every MVCC version of user keys in
/// `[start, end)`.
///
/// Internal keys are `user_key || inverted_commit_ts` (8 bytes), so versions of
/// a user key that is a *proper prefix* of `end` can sort at or above `end`.
/// One extra window per proper prefix of `end` closes that gap; the exact
/// per-user-key filter runs during ingestion.
fn scan_windows(start: &[u8], end: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    if end.is_empty() {
        // All-0xFF prefix boundary: unbounded above.
        return vec![(start.to_vec(), vec![0xFF; 64])];
    }
    let mut windows = vec![(start.to_vec(), end.to_vec())];
    for plen in 1..end.len() {
        let p = &end[..plen];
        if p >= start && p < end {
            // `p || 0xFF * 9` bounds all internal keys of exactly `p`
            // (the MVCC suffix is 8 bytes, so 9 bytes of 0xFF dominate).
            let mut hi = p.to_vec();
            hi.extend_from_slice(&[0xFF; 9]);
            windows.push((p.to_vec(), hi));
        }
    }
    windows
}

impl StorageEngine for LsmTree {
    type Error = StorageError;

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        LsmTree::get(self, key)
    }

    fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        LsmTree::put(self, key, value)
    }

    fn delete(&mut self, key: &[u8]) -> Result<bool, Self::Error> {
        LsmTree::delete(self, key)
    }

    fn iter(&self) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + '_ {
        self.merge_visible(None).into_iter().filter_map(|(k, ver)| {
            if ver.deleted {
                None
            } else {
                Some((k, ver.value))
            }
        })
    }

    fn range<'a>(
        &'a self,
        start: &'a [u8],
        end: &'a [u8],
    ) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> + 'a {
        self.merge_visible(Some((start, end)))
            .into_iter()
            .filter_map(|(k, ver)| {
                if ver.deleted {
                    None
                } else {
                    Some((k, ver.value))
                }
            })
    }

    fn range_keys<'a>(
        &'a self,
        start: &'a [u8],
        end: &'a [u8],
    ) -> impl Iterator<Item = Vec<u8>> + 'a {
        let view = crate::mvcc::ReadView::new(0, u64::MAX, std::collections::BTreeSet::new());
        self.range_visible_keys(start, end, &view)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("noedb-lsm-{prefix}-{nanos}"))
    }

    #[test]
    fn crash_recovery_replays_wal_on_reopen() {
        let dir = temp_dir("crash");
        {
            let mut tree = LsmTree::open(
                &dir,
                LsmConfig {
                    max_mem_bytes: 1_000_000,
                    l0_compaction_trigger: 99,
                    wal_sync: WalSyncMode::EveryAppend,
                    ..Default::default()
                },
            )
            .unwrap();
            tree.put(b"persist", b"yes").unwrap();
        }
        let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        assert_eq!(tree.get(b"persist").unwrap(), Some(b"yes".to_vec()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn on_flush_sync_makes_batch_durable() {
        let dir = temp_dir("batch");
        {
            let mut tree = LsmTree::open(&dir, LsmConfig::throughput()).unwrap();
            tree.put_batch(&[(b"k", b"v")]).unwrap();
        }
        let tree = LsmTree::open(&dir, LsmConfig::default()).unwrap();
        assert_eq!(tree.get(b"k").unwrap(), Some(b"v".to_vec()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn flush_creates_l0_sstable() {
        let dir = temp_dir("flush");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 48,
                l0_compaction_trigger: 99,
                wal_sync: WalSyncMode::EveryAppend,
                ..Default::default()
            },
        )
        .unwrap();
        tree.put(
            b"0123456789012345678901234567890123456789012345678901234567890",
            b"x",
        )
        .unwrap();
        assert!(tree.l0_count() >= 1);
        assert_eq!(
            tree.get(b"0123456789012345678901234567890123456789012345678901234567890")
                .unwrap(),
            Some(b"x".to_vec())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ten_k_puts_readable() {
        let dir = temp_dir("10k");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 4096,
                l0_compaction_trigger: 4,
                wal_sync: WalSyncMode::OnFlush,
                ..Default::default()
            },
        )
        .unwrap();

        for i in 0..10_000u32 {
            let k = format!("row:{i:05}");
            tree.put(k.as_bytes(), b"data").unwrap();
        }
        tree.sync().unwrap();
        for i in (0..10_000u32).step_by(10) {
            let k = format!("row:{i:05}");
            assert_eq!(tree.get(k.as_bytes()).unwrap(), Some(b"data".to_vec()));
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn newer_memtable_write_shadows_flushed_sst_copy() {
        let dir = temp_dir("shadow");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 64,
                l0_compaction_trigger: 99,
                wal_sync: WalSyncMode::EveryAppend,
                ..Default::default()
            },
        )
        .unwrap();
        // First write is flushed to an L0 SST by the tiny memtable budget.
        tree.put(b"user:1", &[b'x'; 100]).unwrap();
        assert!(tree.l0_count() >= 1);
        // Second write stays in the active memtable and must win the read.
        tree.put(b"user:1", b"new").unwrap();
        assert_eq!(tree.get(b"user:1").unwrap(), Some(b"new".to_vec()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_reaches_keys_already_flushed_to_sst() {
        let dir = temp_dir("del-flushed");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 64,
                l0_compaction_trigger: 99,
                wal_sync: WalSyncMode::EveryAppend,
                ..Default::default()
            },
        )
        .unwrap();
        tree.put(b"user:1", &[b'y'; 100]).unwrap();
        assert!(tree.l0_count() >= 1, "value must live in an SST");

        assert!(tree.delete(b"user:1").unwrap());
        assert_eq!(tree.get(b"user:1").unwrap(), None);
        assert!(
            !StorageEngine::iter(&tree).any(|(k, _)| k == b"user:1"),
            "deleted key must not resurrect in scans"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn range_matches_filtered_iter_across_ssts_and_memtable() {
        let dir = temp_dir("range");
        let mut tree = LsmTree::open(
            &dir,
            LsmConfig {
                max_mem_bytes: 4096,
                l0_compaction_trigger: 4,
                wal_sync: WalSyncMode::OnFlush,
                ..Default::default()
            },
        )
        .unwrap();

        // Two "tables" interleaved, spilled over several SSTs + active memtable.
        for i in 0..2_000u32 {
            let ka = format!("alpha\0{i:05}\0col");
            let kb = format!("beta\0{i:05}\0col");
            tree.put(ka.as_bytes(), b"a").unwrap();
            tree.put(kb.as_bytes(), b"b").unwrap();
        }
        // MVCC versions on top (newest must win).
        tree.put_version(
            b"alpha\x0000042\0col",
            &crate::mvcc::Version::put(9, b"a2".to_vec()),
        )
        .unwrap();

        let mut prefix = b"alpha".to_vec();
        prefix.push(0);
        let end = crate::engine::prefix_end(&prefix);

        let ranged: Vec<_> = StorageEngine::range(&tree, &prefix, &end).collect();
        let filtered: Vec<_> = StorageEngine::iter(&tree)
            .filter(|(k, _)| k.starts_with(&prefix))
            .collect();
        assert_eq!(ranged, filtered);
        assert_eq!(ranged.len(), 2_000);
        let updated = ranged
            .iter()
            .find(|(k, _)| k == b"alpha\x0000042\0col")
            .unwrap();
        assert_eq!(updated.1, b"a2".to_vec());
        let _ = fs::remove_dir_all(dir);
    }
}
