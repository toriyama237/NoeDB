//! Background disk scrubber — proactive bit-rot detection on SSTables.

use std::path::Path;

use crate::error::StorageError;
use crate::sstable::SstReader;

/// Result of a full data-directory scrub pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrubReport {
    /// SST files opened successfully.
    pub files_scanned: usize,
    /// Data blocks whose checksums verified.
    pub blocks_verified: usize,
    /// Blocks that failed checksum verification.
    pub checksum_failures: usize,
}

/// Walk `data_dir/sst` and verify every SSTable block checksum.
pub fn scrub_data_dir(data_dir: &Path) -> Result<ScrubReport, StorageError> {
    let sst_dir = data_dir.join("sst");
    if !sst_dir.exists() {
        return Ok(ScrubReport::default());
    }

    let mut report = ScrubReport::default();
    for entry in std::fs::read_dir(&sst_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sst") {
            continue;
        }
        match scrub_sst(&path) {
            Ok(blocks) => {
                report.files_scanned += 1;
                report.blocks_verified += blocks;
            }
            Err(StorageError::ChecksumMismatch) => {
                report.files_scanned += 1;
                report.checksum_failures += 1;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(report)
}

fn scrub_sst(path: &Path) -> Result<usize, StorageError> {
    let reader = SstReader::open(path)?;
    reader.verify_all_blocks()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::memtable::MemTable;
    use crate::sstable::SstWriter;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn scrub_clean_directory() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("noedb-scrub-{nanos}"));
        let sst_dir = dir.join("sst");
        std::fs::create_dir_all(&sst_dir).unwrap();
        let mut table = MemTable::new();
        table.put(b"k", b"v").unwrap();
        let path = sst_dir.join("00000000000000000001.sst");
        SstWriter::write_from_memtable(&path, &table).unwrap();
        let report = scrub_data_dir(&dir).unwrap();
        assert_eq!(report.files_scanned, 1);
        assert!(report.blocks_verified >= 1);
        assert_eq!(report.checksum_failures, 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
