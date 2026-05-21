//! SSTable on-disk format constants.

/// SSTable magic bytes (`NOES` = NoeDB SST).
pub const SST_MAGIC: [u8; 4] = *b"NOES";

/// Current SSTable format version.
pub const SST_VERSION: u16 = 1;

/// Target data block size in bytes.
pub(super) const BLOCK_SIZE: usize = 4096;

/// File header length.
pub(super) const HEADER_LEN: usize = 16;

/// Footer length at EOF (fixed layout).
pub(super) const FOOTER_LEN: usize = 32;

/// One sparse index entry pointing at a data block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexEntry {
    /// First key in the block (for binary search).
    pub first_key: Vec<u8>,
    /// Absolute file offset of the block.
    pub offset: u64,
}

/// Footer parsed from the end of an SSTable file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Footer {
    /// Byte offset of the index section.
    pub index_offset: u64,
    /// Number of index entries.
    pub index_count: u32,
    /// Byte offset of the bloom filter section.
    pub bloom_offset: u64,
}
