//! Buffered zero-copy SSTable reads (Phase 3 Week 16).
//!
//! Loads the SST file once into an `Arc<[u8]>` so block reads are slice views without
//! per-lookup `open`/`seek` (same access pattern as `mmap`, without `unsafe`).

use std::path::Path;
use std::sync::Arc;

use crate::error::StorageError;

/// Read-only SST payload cached in memory.
#[derive(Debug, Clone)]
pub struct MappedFile(Arc<[u8]>);

impl MappedFile {
    /// Underlying bytes.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

/// Read a compressed block from a mapped SSTable buffer.
///
/// Returns an owned buffer (one copy from the mapping).
pub fn read_block_from_mmap(mmap: &MappedFile, offset: u64) -> Result<Vec<u8>, StorageError> {
    read_block_from_slice(mmap.as_slice(), offset)
}

fn read_block_from_slice(data: &[u8], offset: u64) -> Result<Vec<u8>, StorageError> {
    let off = usize::try_from(offset)
        .map_err(|_| StorageError::corrupt_sstable("block offset overflow"))?;
    if off.saturating_add(4) > data.len() {
        return Err(StorageError::corrupt_sstable(
            "block length header past EOF",
        ));
    }
    let len = u32::from_le_bytes(
        data[off..off + 4]
            .try_into()
            .map_err(|_| StorageError::corrupt_sstable("bad block length"))?,
    ) as usize;
    let start = off + 4;
    let end = start
        .checked_add(len)
        .ok_or_else(|| StorageError::corrupt_sstable("block length overflow"))?;
    if end > data.len() {
        return Err(StorageError::corrupt_sstable("block past EOF"));
    }
    Ok(data[start..end].to_vec())
}

/// Load an on-disk SSTable into a shared read-only buffer.
///
/// # Errors
///
/// IO failures.
pub fn map_read_only(path: impl AsRef<Path>) -> Result<MappedFile, StorageError> {
    let bytes = std::fs::read(path.as_ref())?;
    Ok(MappedFile(Arc::from(bytes.into_boxed_slice())))
}
