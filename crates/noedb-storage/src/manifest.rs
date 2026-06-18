//! Atomic LSM manifest — crash-safe SST level inventory.

use std::path::{Path, PathBuf};

use crate::atomic_io::atomic_write;
use crate::error::StorageError;

const MANIFEST: &str = "MANIFEST";

/// On-disk snapshot of L0/L1 SST paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSnapshot {
    /// L0 SST paths (relative to data dir).
    pub level0: Vec<PathBuf>,
    /// L1 SST paths (relative to data dir).
    pub level1: Vec<PathBuf>,
    /// Monotonic sequence for debugging / scrub ordering.
    pub sequence: u64,
}

impl ManifestSnapshot {
    /// Load from `data_dir/MANIFEST` if present.
    pub fn load(data_dir: &Path) -> Result<Option<Self>, StorageError> {
        let path = data_dir.join(MANIFEST);
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)?;
        parse(&raw)
    }

    /// Atomically persist this snapshot.
    pub fn commit(&self, data_dir: &Path) -> Result<(), StorageError> {
        let body = encode(self);
        atomic_write(&data_dir.join(MANIFEST), body.as_bytes())
    }
}

fn encode(snap: &ManifestSnapshot) -> String {
    let l0: Vec<_> = snap
        .level0
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let l1: Vec<_> = snap
        .level1
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    format!(
        "seq={}\nlevel0={}\nlevel1={}\n",
        snap.sequence,
        l0.join(","),
        l1.join(",")
    )
}

fn parse(raw: &str) -> Result<Option<ManifestSnapshot>, StorageError> {
    let mut seq = 0u64;
    let mut l0 = Vec::new();
    let mut l1 = Vec::new();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("seq=") {
            seq = rest
                .trim()
                .parse()
                .map_err(|_| StorageError::corrupt_sstable("bad manifest seq"))?;
        } else if let Some(rest) = line.strip_prefix("level0=") {
            l0 = split_paths(rest);
        } else if let Some(rest) = line.strip_prefix("level1=") {
            l1 = split_paths(rest);
        }
    }
    Ok(Some(ManifestSnapshot {
        level0: l0,
        level1: l1,
        sequence: seq,
    }))
}

fn split_paths(raw: &str) -> Vec<PathBuf> {
    raw.split(',')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn manifest_atomic_commit_reload() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("noedb-manifest-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let snap = ManifestSnapshot {
            level0: vec![PathBuf::from("sst/l0-001.sst")],
            level1: vec![PathBuf::from("sst/l1-001.sst")],
            sequence: 42,
        };
        snap.commit(&dir).unwrap();
        let loaded = ManifestSnapshot::load(&dir).unwrap().unwrap();
        assert_eq!(loaded, snap);
        let _ = std::fs::remove_dir_all(dir);
    }
}
