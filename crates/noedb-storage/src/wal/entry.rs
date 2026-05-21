//! WAL record types and binary codec.

use crate::checksum::crc32;
use crate::error::StorageError;

/// WAL file magic bytes (`NOEW` = NoeDB WAL).
pub const WAL_MAGIC: [u8; 4] = *b"NOEW";

/// Current on-disk WAL format version.
pub const WAL_VERSION: u16 = 1;

/// Operation stored in the write-ahead log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpType {
    /// Insert or overwrite a key.
    Put,
    /// Remove a key (tombstone).
    Delete,
}

impl OpType {
    const fn to_byte(self) -> u8 {
        match self {
            Self::Put => 0,
            Self::Delete => 1,
        }
    }

    const fn from_byte(b: u8) -> Result<Self, StorageError> {
        match b {
            0 => Ok(Self::Put),
            1 => Ok(Self::Delete),
            _ => Err(StorageError::corrupt_wal("unknown OpType tag")),
        }
    }
}

/// One durable log record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    /// Put or delete.
    pub op: OpType,
    /// Record key (non-empty).
    pub key: Vec<u8>,
    /// Value for [`OpType::Put`]; empty for deletes.
    pub value: Vec<u8>,
}

impl LogEntry {
    /// Construct a put record.
    #[must_use]
    pub fn put(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            op: OpType::Put,
            key,
            value,
        }
    }

    /// Construct a delete tombstone.
    #[must_use]
    pub fn delete(key: Vec<u8>) -> Self {
        Self {
            op: OpType::Delete,
            key,
            value: Vec::new(),
        }
    }

    /// Encode to on-disk bytes: `| u32 len | u32 crc | payload |`.
    pub fn encode(&self) -> Result<Vec<u8>, StorageError> {
        if self.key.is_empty() {
            return Err(StorageError::invalid_input("WAL key must not be empty"));
        }
        if self.op == OpType::Delete && !self.value.is_empty() {
            return Err(StorageError::invalid_input(
                "delete records must not carry a value",
            ));
        }

        let mut payload = Vec::with_capacity(1 + 8 + self.key.len() + self.value.len());
        payload.push(self.op.to_byte());
        payload.extend_from_slice(&(self.key.len() as u32).to_le_bytes());
        payload.extend_from_slice(&self.key);
        payload.extend_from_slice(&(self.value.len() as u32).to_le_bytes());
        payload.extend_from_slice(&self.value);

        let checksum = crc32(&payload);
        let record_len = 4u32 + payload.len() as u32;

        let mut out = Vec::with_capacity(4 + record_len as usize);
        out.extend_from_slice(&record_len.to_le_bytes());
        out.extend_from_slice(&checksum.to_le_bytes());
        out.extend_from_slice(&payload);
        Ok(out)
    }

    /// Decode one record from `data` (starting at the length prefix).
    pub fn decode(data: &[u8]) -> Result<(Self, usize), StorageError> {
        if data.len() < 4 {
            return Err(StorageError::corrupt_wal("truncated record length"));
        }
        let record_len = read_u32(data, 0)? as usize;
        if data.len() < 4 + record_len {
            return Err(StorageError::corrupt_wal("truncated record body"));
        }
        let record = &data[4..4 + record_len];
        let consumed = 4 + record_len;

        if record.len() < 4 {
            return Err(StorageError::corrupt_wal("missing checksum"));
        }
        let expected_crc = read_u32(record, 0)?;
        let payload = &record[4..];
        if crc32(payload) != expected_crc {
            return Err(StorageError::checksum_mismatch());
        }

        Self::decode_payload(payload).map(|entry| (entry, consumed))
    }

    fn decode_payload(payload: &[u8]) -> Result<Self, StorageError> {
        if payload.is_empty() {
            return Err(StorageError::corrupt_wal("empty payload"));
        }
        let op = OpType::from_byte(payload[0])?;
        let mut off = 1usize;

        let key = read_len_prefixed(payload, &mut off)?;
        let value = read_len_prefixed(payload, &mut off)?;

        if key.is_empty() {
            return Err(StorageError::corrupt_wal("empty key in WAL record"));
        }
        if op == OpType::Delete && !value.is_empty() {
            return Err(StorageError::corrupt_wal("delete with non-empty value"));
        }

        Ok(Self { op, key, value })
    }
}

fn read_u32(data: &[u8], off: usize) -> Result<u32, StorageError> {
    let bytes: [u8; 4] = data
        .get(off..off + 4)
        .ok_or(StorageError::corrupt_wal("truncated u32"))?
        .try_into()
        .map_err(|_| StorageError::corrupt_wal("truncated u32"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_len_prefixed(data: &[u8], off: &mut usize) -> Result<Vec<u8>, StorageError> {
    let len = read_u32(data, *off)? as usize;
    *off += 4;
    if *off + len > data.len() {
        return Err(StorageError::corrupt_wal("truncated field"));
    }
    let slice = data[*off..*off + len].to_vec();
    *off += len;
    Ok(slice)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip_put() {
        let entry = LogEntry::put(b"k".to_vec(), b"v".to_vec());
        let bytes = entry.encode().unwrap();
        let (decoded, n) = LogEntry::decode(&bytes).unwrap();
        assert_eq!(decoded, entry);
        assert_eq!(n, bytes.len());
    }

    #[test]
    fn encode_decode_round_trip_delete() {
        let entry = LogEntry::delete(b"k".to_vec());
        let bytes = entry.encode().unwrap();
        let (decoded, _) = LogEntry::decode(&bytes).unwrap();
        assert_eq!(decoded, entry);
    }

    #[test]
    fn tampered_checksum_is_rejected() {
        let mut bytes = LogEntry::put(b"k".to_vec(), b"v".to_vec())
            .encode()
            .unwrap();
        bytes[5] ^= 0xFF;
        assert_eq!(
            LogEntry::decode(&bytes).unwrap_err(),
            StorageError::checksum_mismatch()
        );
    }
}
