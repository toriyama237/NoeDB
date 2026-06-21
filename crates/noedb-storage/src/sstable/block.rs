//! SSTable data block encode/decode with optional CRC32C trailer.

use crate::checksum::crc32c;
use crate::error::StorageError;

/// Legacy v1/v2 on-disk layout: `[len: u32][payload]`.
pub(crate) const LEGACY_BLOCK_HEADER: usize = 4;
/// v3 layout: `[len: u32][crc32c: u32][payload]`.
pub(crate) const CHECKSUM_BLOCK_HEADER: usize = 8;

/// Serialize a block for v3 SSTables.
pub(crate) fn encode_block(payload: &[u8]) -> Vec<u8> {
    let checksum = crc32c(payload);
    let mut out = Vec::with_capacity(CHECKSUM_BLOCK_HEADER + payload.len());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&checksum.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

/// Decode a block at `offset` within `file_bytes`.
pub(crate) fn decode_block_at(
    data: &[u8],
    offset: u64,
    checksum_blocks: bool,
) -> Result<Vec<u8>, StorageError> {
    let off = usize::try_from(offset)
        .map_err(|_| StorageError::corrupt_sstable("block offset overflow"))?;
    let header = if checksum_blocks {
        CHECKSUM_BLOCK_HEADER
    } else {
        LEGACY_BLOCK_HEADER
    };
    if off.saturating_add(header) > data.len() {
        return Err(StorageError::corrupt_sstable(
            "block length header past EOF",
        ));
    }
    let len = u32::from_le_bytes(
        data[off..off + 4]
            .try_into()
            .map_err(|_| StorageError::corrupt_sstable("bad block length"))?,
    ) as usize;
    let payload_start = if checksum_blocks {
        let checksum = u32::from_le_bytes(
            data[off + 4..off + 8]
                .try_into()
                .map_err(|_| StorageError::corrupt_sstable("bad block checksum"))?,
        );
        let start = off + CHECKSUM_BLOCK_HEADER;
        let end = start
            .checked_add(len)
            .ok_or_else(|| StorageError::corrupt_sstable("block length overflow"))?;
        if end > data.len() {
            return Err(StorageError::corrupt_sstable("block past EOF"));
        }
        let payload = &data[start..end];
        if crc32c(payload) != checksum {
            return Err(StorageError::checksum_mismatch());
        }
        return Ok(payload.to_vec());
    } else {
        off + LEGACY_BLOCK_HEADER
    };
    let end = payload_start
        .checked_add(len)
        .ok_or_else(|| StorageError::corrupt_sstable("block length overflow"))?;
    if end > data.len() {
        return Err(StorageError::corrupt_sstable("block past EOF"));
    }
    Ok(data[payload_start..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_round_trip() {
        let payload = b"noedb-block-payload";
        let encoded = encode_block(payload);
        let decoded = decode_block_at(&encoded, 0, true).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn tampered_checksum_rejected() {
        let mut encoded = encode_block(b"secret");
        encoded[7] ^= 0xFF;
        assert!(matches!(
            decode_block_at(&encoded, 0, true),
            Err(StorageError::ChecksumMismatch)
        ));
    }
}
