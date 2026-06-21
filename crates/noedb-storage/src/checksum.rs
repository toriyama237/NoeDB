//! CRC-32 (IEEE) for WAL records; CRC32C (Castagnoli) for SSTable blocks.

const POLY: u32 = 0xEDB8_8320;

/// Incremental CRC-32 (IEEE) hasher.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Crc32 {
    state: u32,
}

impl Crc32 {
    /// New hasher with the standard CRC-32 initial value.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self { state: 0xFFFF_FFFF }
    }

    /// Feed bytes into the hasher.
    pub(crate) fn update(&mut self, data: &[u8]) {
        for &byte in data {
            self.state ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (self.state & 1).wrapping_neg();
                self.state = (self.state >> 1) ^ (POLY & mask);
            }
        }
    }

    /// Finalize to the on-wire checksum value.
    #[must_use]
    pub(crate) const fn finish(self) -> u32 {
        !self.state
    }
}

/// One-shot CRC-32 (IEEE) over `data`.
#[must_use]
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut h = Crc32::new();
    h.update(data);
    h.finish()
}

/// One-shot CRC32C (Castagnoli) for SSTable block integrity — same polynomial as RocksDB/Pebble.
#[must_use]
pub(crate) fn crc32c(data: &[u8]) -> u32 {
    crc32c::crc32c(data)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_known_vector() {
        assert_eq!(crc32(b""), 0);
    }

    #[test]
    fn incremental_matches_one_shot() {
        let data = b"noedb-wal-entry";
        let mut h = Crc32::new();
        h.update(&data[..5]);
        h.update(&data[5..]);
        assert_eq!(h.finish(), crc32(data));
    }

    #[test]
    fn crc32c_detects_single_bit_flip() {
        let data = b"noedb-sst-block";
        let good = crc32c(data);
        assert_ne!(good, crc32c(&[data[0] ^ 0x01]));
    }
}
