//! LZ4 block compression for SSTables (Phase 3 Week 19).

use lz4_flex::{compress_prepend_size, decompress_size_prepended};

use crate::error::StorageError;

/// Block is stored uncompressed.
pub const BLOCK_RAW: u8 = 0;
/// Block payload is LZ4-compressed (`lz4_flex` prepend-size format).
pub const BLOCK_LZ4: u8 = 1;

/// Compress `data` when it shrinks; otherwise return raw block with header byte.
pub fn maybe_compress_block(data: &[u8]) -> Vec<u8> {
    if data.len() < 64 {
        let mut out = Vec::with_capacity(1 + data.len());
        out.push(BLOCK_RAW);
        out.extend_from_slice(data);
        return out;
    }
    let compressed = compress_prepend_size(data);
    if compressed.len() + 1 < data.len() {
        let mut out = Vec::with_capacity(1 + compressed.len());
        out.push(BLOCK_LZ4);
        out.extend_from_slice(&compressed);
        out
    } else {
        let mut out = Vec::with_capacity(1 + data.len());
        out.push(BLOCK_RAW);
        out.extend_from_slice(data);
        out
    }
}

/// Decode on-disk block (raw or LZ4).
pub fn decompress_block(stored: &[u8]) -> Result<Vec<u8>, StorageError> {
    if stored.is_empty() {
        return Err(StorageError::corrupt_sstable("empty SST block"));
    }
    match stored[0] {
        BLOCK_RAW => Ok(stored[1..].to_vec()),
        BLOCK_LZ4 => decompress_size_prepended(&stored[1..]).map_err(|e| StorageError::Io {
            message: format!("LZ4 decompress: {e}"),
        }),
        _ => Err(StorageError::corrupt_sstable("unknown SST block codec")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_compresses_repetitive_data() {
        let data = vec![b'x'; 4096];
        let stored = maybe_compress_block(&data);
        assert_eq!(stored[0], BLOCK_LZ4);
        let back = decompress_block(&stored).unwrap();
        assert_eq!(back, data);
    }
}
