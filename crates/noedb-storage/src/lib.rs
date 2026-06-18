//! LSM-tree storage engine for NoeDB.
//!
//! Phase 2 (Weeks 09–16) delivers a complete single-node LSM stack:
//!
//! - [`MemTable`] — in-memory sorted write buffer.
//! - [`Wal`] / [`WalSegmentManager`] — durable WAL with segment rotation.
//! - [`SstWriter`] / [`SstReader`] — on-disk SSTables with block index + Bloom filter.
//! - [`LsmTree`] — orchestrates flush, compaction, and crash recovery.
//!
//! See `docs/sprint-plan.md`, Phase 2.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
#![allow(
    clippy::missing_const_for_fn,
    clippy::use_self,
    clippy::explicit_iter_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod atomic_io;
mod bloom;
mod checksum;
mod compaction;
pub mod compress;
mod durable;
mod engine;
mod error;
mod fast_wal;
mod hnsw;
mod lsm;
mod lsm_mvcc;
mod manifest;
mod memtable;
mod mmap_io;
pub mod mvcc;
mod scrub;
mod sstable;
mod wal;
mod write_stall;
mod xor_filter;

pub use crate::bloom::BloomFilter;
pub use crate::compaction::{compact_level0_to_l1, L0_COMPACTION_TRIGGER};
pub use crate::durable::DurableStore;
pub use crate::engine::StorageEngine;
pub use crate::error::StorageError;
pub use crate::fast_wal::append_batch_sync;
pub use crate::lsm::{LsmConfig, LsmTree};
pub use crate::manifest::ManifestSnapshot;
pub use crate::memtable::{
    MemTable, MemTableIter, MemTableRangeIter, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_MEM_BYTES,
};
pub use crate::mmap_io::{map_read_only, read_block_from_mmap, MappedFile};
pub use crate::mvcc::{
    encode_internal_key, gc_versions, CommitTs, GcStats, MvccMemTable, ReadView, SnapshotStore,
    TimestampOracle, TxnId, Version,
};
pub use crate::scrub::{scrub_data_dir, ScrubReport};
pub use crate::sstable::{
    SstReader, SstWriteOptions, SstWriter, SST_MAGIC, SST_VERSION, SST_VERSION_V2, SST_VERSION_V3,
};
pub use crate::wal::{
    replay_into_memtable, replay_wal_dir, LogEntry, OpType, Wal, WalSegmentManager, WalSyncMode,
    WAL_MAGIC, WAL_VERSION,
};
pub use crate::hnsw::HnswIndex;
pub use crate::write_stall::{WriteStallConfig, WriteStallController};
pub use crate::xor_filter::XorFilter;
