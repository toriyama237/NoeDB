//! SSTable write options (Phase 3).

use super::format::{SST_FLAG_LZ4_BLOCKS, SST_FLAG_XOR_FILTER, SST_VERSION, SST_VERSION_V2};

/// Controls on-disk SST features (Weeks 19–20).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SstWriteOptions {
    /// LZ4-compress data blocks when beneficial.
    pub compress_blocks: bool,
    /// Use XOR filter on SST v2 (default: Bloom on v2 for reliability).
    pub xor_filter: bool,
}

impl Default for SstWriteOptions {
    fn default() -> Self {
        Self::phase3_default()
    }
}

impl SstWriteOptions {
    /// Phase 3 default: SST v2 + LZ4 blocks + Bloom filter.
    #[must_use]
    pub const fn phase3_default() -> Self {
        Self {
            compress_blocks: true,
            xor_filter: false,
        }
    }

    /// Experimental: SST v2 + LZ4 + XOR filter (small key sets only).
    #[must_use]
    pub const fn with_xor_filter() -> Self {
        Self {
            compress_blocks: true,
            xor_filter: true,
        }
    }

    /// Legacy v1 SST (Bloom, raw blocks).
    #[must_use]
    pub const fn legacy() -> Self {
        Self {
            compress_blocks: false,
            xor_filter: false,
        }
    }

    /// File format version to write.
    #[must_use]
    pub const fn version(self) -> u16 {
        if self.compress_blocks || self.xor_filter {
            SST_VERSION_V2
        } else {
            SST_VERSION
        }
    }

    /// Header feature flags (v2).
    #[must_use]
    pub const fn header_flags(self) -> u16 {
        let mut f = 0u16;
        if self.compress_blocks {
            f |= SST_FLAG_LZ4_BLOCKS;
        }
        if self.xor_filter {
            f |= SST_FLAG_XOR_FILTER;
        }
        f
    }
}
