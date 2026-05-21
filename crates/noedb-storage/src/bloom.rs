//! Bloom filter built from scratch (Week 14).

use crate::checksum::crc32;

/// A classic Bloom filter using double hashing for `k` probe functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BloomFilter {
    bits: Vec<u8>,
    m: u32,
    k: u32,
}

impl BloomFilter {
    /// Build a filter for `expected_keys` with target false-positive rate `p`.
    #[must_use]
    pub fn build(
        keys: impl IntoIterator<Item = impl AsRef<[u8]>>,
        expected_keys: usize,
        p: f64,
    ) -> Self {
        let n_keys = u32::try_from(expected_keys.max(1)).unwrap_or(u32::MAX);
        let n = f64::from(n_keys);
        let ln2 = core::f64::consts::LN_2;
        let m = ((-n * p.ln()) / (ln2 * ln2))
            .ceil()
            .max(64.0)
            .min(f64::from(u32::MAX)) as u32;
        let k = ((f64::from(m) / n) * ln2)
            .round()
            .clamp(1.0, 30.0)
            .min(f64::from(u32::MAX)) as u32;
        let mut filter = Self {
            bits: vec![0; (m as usize).div_ceil(8)],
            m,
            k,
        };
        for key in keys {
            filter.insert(key.as_ref());
        }
        filter
    }

    /// Insert a key into the filter.
    pub fn insert(&mut self, key: &[u8]) {
        let (h1, h2) = hash_pair(key);
        for i in 0..self.k {
            let idx = double_hash(h1, h2, i, self.m);
            set_bit(&mut self.bits, idx);
        }
    }

    /// Returns `false` if the key is definitely absent.
    #[must_use]
    pub fn may_contain(&self, key: &[u8]) -> bool {
        let (h1, h2) = hash_pair(key);
        for i in 0..self.k {
            let idx = double_hash(h1, h2, i, self.m);
            if !get_bit(&self.bits, idx) {
                return false;
            }
        }
        true
    }

    /// Serialize to bytes: `m(4) | k(1) | padding(3) | bits`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.bits.len());
        out.extend_from_slice(&self.m.to_le_bytes());
        out.push(self.k as u8);
        out.extend_from_slice(&[0u8; 3]);
        out.extend_from_slice(&self.bits);
        out
    }

    /// Decode from [`Self::encode`] output.
    pub fn decode(data: &[u8]) -> Result<Self, crate::error::StorageError> {
        if data.len() < 8 {
            return Err(crate::error::StorageError::corrupt_sstable(
                "truncated bloom filter",
            ));
        }
        let m =
            u32::from_le_bytes(data[0..4].try_into().map_err(|_| {
                crate::error::StorageError::corrupt_sstable("truncated bloom filter")
            })?);
        let k = u32::from(data[4]);
        let bits = data[8..].to_vec();
        Ok(Self { bits, m, k })
    }

    /// Number of bits in the filter.
    #[must_use]
    pub const fn bit_count(&self) -> u32 {
        self.m
    }

    /// Number of hash functions.
    #[must_use]
    pub const fn hash_count(&self) -> u32 {
        self.k
    }
}

fn hash_pair(key: &[u8]) -> (u32, u32) {
    let h1 = crc32(key);
    let mut buf = h1.to_le_bytes().to_vec();
    buf.extend_from_slice(key);
    let h2 = crc32(&buf) | 1;
    (h1, h2)
}

fn double_hash(h1: u32, h2: u32, i: u32, m: u32) -> u32 {
    (h1.wrapping_add(i.wrapping_mul(h2))) % m
}

fn set_bit(bits: &mut [u8], idx: u32) {
    let byte = (idx / 8) as usize;
    let bit = idx % 8;
    if byte < bits.len() {
        bits[byte] |= 1 << bit;
    }
}

fn get_bit(bits: &[u8], idx: u32) -> bool {
    let byte = (idx / 8) as usize;
    let bit = idx % 8;
    byte < bits.len() && (bits[byte] & (1 << bit)) != 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn false_positive_rate_under_one_percent() {
        let keys: Vec<Vec<u8>> = (0..10_000u32)
            .map(|i| format!("key:{i}").into_bytes())
            .collect();
        let filter = BloomFilter::build(keys.iter(), 10_000, 0.01);
        for k in &keys {
            assert!(filter.may_contain(k));
        }
        let mut false_positives = 0u32;
        for i in 10_000..20_000u32 {
            let probe = format!("probe:{i}").into_bytes();
            if filter.may_contain(&probe) {
                false_positives += 1;
            }
        }
        let rate = f64::from(false_positives) / 10_000.0;
        assert!(rate < 0.02, "FPR {rate} >= 2%");
    }

    #[test]
    fn encode_decode_round_trip() {
        let filter = BloomFilter::build([b"a", b"b"], 2, 0.01);
        let decoded = BloomFilter::decode(&filter.encode()).unwrap();
        assert_eq!(decoded, filter);
    }
}
