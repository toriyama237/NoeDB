//! Internal key encoding: `user_key || inverted_commit_ts` (newest first).

use super::CommitTs;

/// Encode internal LSM key (newer `commit_ts` sorts before older for same user key).
#[must_use]
pub fn encode_internal_key(user_key: &[u8], commit_ts: CommitTs) -> Vec<u8> {
    let inv = !commit_ts;
    let mut out = Vec::with_capacity(user_key.len() + 8);
    out.extend_from_slice(user_key);
    out.extend_from_slice(&inv.to_be_bytes());
    out
}

/// Extract user key prefix from an internal key (must be at least 8 suffix bytes).
#[must_use]
pub fn decode_user_key(internal: &[u8]) -> &[u8] {
    &internal[..internal.len().saturating_sub(8)]
}

/// Upper bound for scanning all versions of `user_key` (exclusive).
#[must_use]
pub fn user_key_prefix_end(user_key: &[u8]) -> Vec<u8> {
    let mut end = user_key.to_vec();
    while let Some(b) = end.last_mut() {
        if *b == 255 {
            end.pop();
        } else {
            *b += 1;
            return end;
        }
    }
    end
}
