//! Packed row records — one storage entry per row (v2.3).
//!
//! The historical layout stored one KV pair per **cell**
//! (`table\0row\0col` → bytes): a 14-column row cost 14 keys, 14 WAL
//! appends, 14 MVCC versions, and scans had to regroup cells into rows.
//! Packed records store the whole row under `table\0row` as a single
//! self-describing value, cutting write amplification and scan work by
//! the column count.
//!
//! Both layouts coexist: readers detect packed records by the magic
//! header and fall back to the legacy cell path otherwise.

/// Magic prefix identifying a packed row record (NoeDB Row Pack v1).
pub const ROWPACK_MAGIC: &[u8; 4] = b"NRP1";

/// Encode `(column, value)` pairs into a packed record.
///
/// Layout: `"NRP1" | u16 count | { u16 name_len | name | u32 val_len | val }*`
/// (little-endian). Column order is preserved.
#[must_use]
pub fn encode_row(columns: &[(String, Vec<u8>)]) -> Vec<u8> {
    let payload: usize = columns.iter().map(|(n, v)| 6 + n.len() + v.len()).sum();
    let mut out = Vec::with_capacity(4 + 2 + payload);
    out.extend_from_slice(ROWPACK_MAGIC);
    let count = u16::try_from(columns.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&count.to_le_bytes());
    for (name, value) in columns.iter().take(usize::from(count)) {
        let name_len = u16::try_from(name.len()).unwrap_or(u16::MAX);
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(&name.as_bytes()[..usize::from(name_len)]);
        let val_len = u32::try_from(value.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&val_len.to_le_bytes());
        out.extend_from_slice(&value[..val_len as usize]);
    }
    out
}

/// Whether `bytes` is a packed row record.
#[must_use]
pub fn is_packed_row(bytes: &[u8]) -> bool {
    bytes.len() >= 6 && &bytes[..4] == ROWPACK_MAGIC
}

/// Decode a packed record into `(column, value)` pairs.
///
/// Returns `None` when the magic is absent or the record is truncated —
/// callers then treat the bytes as a legacy single-cell value.
#[must_use]
pub fn decode_row(bytes: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    if !is_packed_row(bytes) {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([bytes[4], bytes[5]]));
    let mut off = 6usize;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        if off + 2 > bytes.len() {
            return None;
        }
        let name_len = usize::from(u16::from_le_bytes([bytes[off], bytes[off + 1]]));
        off += 2;
        if off + name_len + 4 > bytes.len() {
            return None;
        }
        let name = String::from_utf8_lossy(&bytes[off..off + name_len]).into_owned();
        off += name_len;
        let val_len =
            u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
                as usize;
        off += 4;
        if off + val_len > bytes.len() {
            return None;
        }
        out.push((name, bytes[off..off + val_len].to_vec()));
        off += val_len;
    }
    Some(out)
}

/// Storage key of a packed row: `table\0row_id`.
#[must_use]
pub fn packed_row_key(table: &str, row_id: &str) -> Vec<u8> {
    let mut key = table.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(row_id.as_bytes());
    key
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_columns_and_order() {
        let cols = vec![
            ("id".to_string(), b"42".to_vec()),
            ("name".to_string(), "Direction Générale".as_bytes().to_vec()),
            ("empty".to_string(), Vec::new()),
        ];
        let packed = encode_row(&cols);
        assert!(is_packed_row(&packed));
        assert_eq!(decode_row(&packed).unwrap(), cols);
    }

    #[test]
    fn legacy_bytes_are_not_packed() {
        assert!(!is_packed_row(b""));
        assert!(!is_packed_row(b"plain cell value"));
        assert!(decode_row(b"NRP").is_none());
    }

    #[test]
    fn truncated_record_is_rejected() {
        let packed = encode_row(&[("col".to_string(), b"value".to_vec())]);
        assert!(decode_row(&packed[..packed.len() - 2]).is_none());
    }
}
