//! On-disk encoding for [`Version`] values in the LSM.

use super::version::Version;
use super::CommitTs;
use crate::error::StorageError;

const MAGIC: &[u8] = b"MVCC";
const FORMAT_V1: u8 = 1;

/// Serialize a committed version for LSM/WAL storage.
pub fn encode_version(version: &Version) -> Result<Vec<u8>, StorageError> {
    if version.commit_ts == 0 {
        return Err(StorageError::invalid_input("cannot persist uncommitted intent"));
    }
    let mut out = Vec::with_capacity(MAGIC.len() + 1 + 8 + 1 + 4 + version.value.len());
    out.extend_from_slice(MAGIC);
    out.push(FORMAT_V1);
    out.extend_from_slice(&version.commit_ts.to_le_bytes());
    out.push(u8::from(version.deleted));
    out.extend_from_slice(&(version.value.len() as u32).to_le_bytes());
    out.extend_from_slice(&version.value);
    Ok(out)
}

/// Decode a version payload from LSM/WAL.
pub fn decode_version(bytes: &[u8]) -> Result<Version, StorageError> {
    if bytes.len() < MAGIC.len() + 1 + 8 + 1 + 4 {
        return Err(StorageError::corrupt_sstable("truncated MVCC value"));
    }
    if &bytes[..MAGIC.len()] != MAGIC {
        return Err(StorageError::corrupt_sstable("bad MVCC magic"));
    }
    if bytes[MAGIC.len()] != FORMAT_V1 {
        return Err(StorageError::corrupt_sstable("unsupported MVCC format"));
    }
    let mut off = MAGIC.len() + 1;
    let commit_ts = read_u64(bytes, &mut off)?;
    let deleted = bytes[off] != 0;
    off += 1;
    let val_len = read_u32(bytes, &mut off)? as usize;
    if off + val_len > bytes.len() {
        return Err(StorageError::corrupt_sstable("truncated MVCC payload"));
    }
    let value = bytes[off..off + val_len].to_vec();
    Ok(Version {
        commit_ts,
        value,
        deleted,
    })
}

/// Plain LSM value (pre-MVCC) — treat as version at ts 1.
pub fn decode_or_legacy(bytes: &[u8]) -> Result<Version, StorageError> {
    if bytes.starts_with(MAGIC) {
        decode_version(bytes)
    } else {
        Ok(Version::put(1, bytes.to_vec()))
    }
}

fn read_u64(data: &[u8], off: &mut usize) -> Result<CommitTs, StorageError> {
    let slice = data
        .get(*off..*off + 8)
        .ok_or(StorageError::corrupt_sstable("truncated u64"))?;
    *off += 8;
    Ok(u64::from_le_bytes(slice.try_into().unwrap()))
}

fn read_u32(data: &[u8], off: &mut usize) -> Result<u32, StorageError> {
    let slice = data
        .get(*off..*off + 4)
        .ok_or(StorageError::corrupt_sstable("truncated u32"))?;
    *off += 4;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_version() {
        let v = Version::put(42, b"data".to_vec());
        let bytes = encode_version(&v).unwrap();
        assert_eq!(decode_version(&bytes).unwrap(), v);
    }
}
