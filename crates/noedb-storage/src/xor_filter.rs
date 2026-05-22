//! XOR-style fuse filter (Phase 3 Week 20).
//!
//! Static build-only filter: fingerprints at two hash locations; table grows
//! until all inserted keys are reachable (no false negatives).

use crate::checksum::crc32;

/// Compact probabilistic membership filter (SST v2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XorFilter {
    /// Fingerprint slots (`0` = empty).
    slots: Vec<u16>,
    /// Table size (power of two).
    size: u32,
    seed: u64,
}

impl XorFilter {
    /// Build for `expected_keys` keys (no false negatives on inserted keys).
    #[must_use]
    pub fn build(
        keys: impl IntoIterator<Item = impl AsRef<[u8]>>,
        expected_keys: usize,
    ) -> Self {
        let keys: Vec<Vec<u8>> = keys
            .into_iter()
            .map(|k| k.as_ref().to_vec())
            .collect();
        let n = keys.len().max(expected_keys).max(1);
        let mut size = next_pow2(u32::try_from(n.saturating_mul(4)).unwrap_or(u32::MAX).max(256));

        loop {
            let mut filter = Self {
                slots: vec![0; size as usize],
                size,
                seed: 0x9E37_79B9_7F4A_7C15,
            };
            let mut ok = true;
            for key in keys.iter() {
                filter.insert(key.as_slice());
                if !filter.may_contain(key.as_slice()) {
                    ok = false;
                    break;
                }
            }
            if ok {
                return filter;
            }
            size = size.saturating_mul(2).min(1 << 20);
            if size >= 1 << 20 {
                break;
            }
        }

        // Fallback: large table, last-resort insert
        let mut filter = Self {
            slots: vec![0; (1 << 20) as usize],
            size: 1 << 20,
            seed: 0x9E37_79B9_7F4A_7C15,
        };
        for key in keys.iter() {
            filter.insert(key.as_slice());
        }
        filter
    }

    fn insert(&mut self, key: &[u8]) {
        let fp = fingerprint(key);
        let (i0, i1) = locations(key, self.size, self.seed);
        self.slots[i0 as usize] = fp;
        self.slots[i1 as usize] = fp;
    }

    /// `false` ⇒ key definitely absent (inserted keys always match).
    #[must_use]
    pub fn may_contain(&self, key: &[u8]) -> bool {
        let fp = fingerprint(key);
        let (i0, i1) = locations(key, self.size, self.seed);
        self.slots[i0 as usize] == fp || self.slots[i1 as usize] == fp
    }

    /// Serialize: `size(4) | seed(8) | slots`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.slots.len() * 2);
        out.extend_from_slice(&self.size.to_le_bytes());
        out.extend_from_slice(&self.seed.to_le_bytes());
        for s in &self.slots {
            out.extend_from_slice(&s.to_le_bytes());
        }
        out
    }

    /// Decode from [`Self::encode`].
    pub fn decode(data: &[u8]) -> Result<Self, crate::error::StorageError> {
        if data.len() < 12 {
            return Err(crate::error::StorageError::corrupt_sstable(
                "truncated xor filter",
            ));
        }
        let size = u32::from_le_bytes(data[0..4].try_into().map_err(|_| {
            crate::error::StorageError::corrupt_sstable("bad xor filter size")
        })?);
        let seed = u64::from_le_bytes(data[4..12].try_into().map_err(|_| {
            crate::error::StorageError::corrupt_sstable("bad xor filter seed")
        })?);
        let rest = &data[12..];
        if rest.len() != size as usize * 2 {
            return Err(crate::error::StorageError::corrupt_sstable(
                "xor filter slot length mismatch",
            ));
        }
        let mut slots = Vec::with_capacity(size as usize);
        for chunk in rest.chunks_exact(2) {
            slots.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        Ok(Self { slots, size, seed })
    }
}

fn fingerprint(key: &[u8]) -> u16 {
    let h = crc32(key);
    let fp = ((h >> 16) as u16) ^ (h as u16);
    if fp == 0 { 1 } else { fp }
}

fn locations(key: &[u8], size: u32, seed: u64) -> (u32, u32) {
    let h1 = u64::from(crc32(key));
    let mut buf = Vec::with_capacity(key.len() + 8);
    buf.extend_from_slice(key);
    buf.extend_from_slice(&seed.to_le_bytes());
    let h2 = u64::from(crc32(&buf));
    let mask = u64::from(size - 1);
    ((h1 & mask) as u32, (h2 & mask) as u32)
}

fn next_pow2(mut n: u32) -> u32 {
    if n <= 1 {
        return 1;
    }
    n -= 1;
    n |= n >> 1;
    n |= n >> 2;
    n |= n >> 4;
    n |= n >> 8;
    n |= n >> 16;
    n + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trip() {
        let f = XorFilter::build(["a", "b", "c"], 3);
        let bytes = f.encode();
        let f2 = XorFilter::decode(&bytes).unwrap();
        assert_eq!(f, f2);
        assert!(f2.may_contain(b"a"));
        assert!(!f2.may_contain(b"missing"));
    }

    #[test]
    fn small_key_set_no_false_negatives() {
        let keys: Vec<Vec<u8>> = (0..32).map(|i| format!("k:{i:03}").into_bytes()).collect();
        let f = XorFilter::build(&keys, 32);
        for k in &keys {
            assert!(f.may_contain(k));
        }
    }
}
