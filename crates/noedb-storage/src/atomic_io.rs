//! Crash-safe file writes: temp + fsync + atomic rename.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use crate::error::StorageError;

/// Write `content` to `path` via a temporary file and atomic rename.
pub(crate) fn atomic_write(path: &Path, content: &[u8]) -> Result<(), StorageError> {
    let parent = path
        .parent()
        .ok_or_else(|| StorageError::invalid_input("atomic write path has no parent"))?;
    fs::create_dir_all(parent)?;
    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp)?;
        file.write_all(content)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn atomic_write_round_trips() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("noedb-atomic-{nanos}.txt"));
        atomic_write(&path, b"manifest-v1").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"manifest-v1");
        let _ = fs::remove_file(path);
    }
}
