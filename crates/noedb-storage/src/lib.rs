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
#![allow(
    clippy::missing_const_for_fn,
    clippy::use_self,
    clippy::explicit_iter_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod bloom;
mod checksum;
mod compaction;
mod durable;
mod engine;
mod error;
mod lsm;
mod lsm_mvcc;
mod memtable;
pub mod mvcc;
mod sstable;
mod wal;

pub use crate::bloom::BloomFilter;
pub use crate::compaction::{compact_level0_to_l1, L0_COMPACTION_TRIGGER};
pub use crate::durable::DurableStore;
pub use crate::engine::StorageEngine;
pub use crate::error::StorageError;
pub use crate::lsm::{LsmConfig, LsmTree};
pub use crate::memtable::{
    MemTable, MemTableIter, MemTableRangeIter, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_MEM_BYTES,
};
pub use crate::sstable::{SstReader, SstWriter, SST_MAGIC, SST_VERSION};
pub use crate::mvcc::{
    gc_versions, encode_internal_key, CommitTs, GcStats, MvccMemTable, ReadView, SnapshotStore,
    TimestampOracle, TxnId, Version,
};
pub use crate::wal::{
    replay_into_memtable, replay_wal_dir, LogEntry, OpType, Wal, WalSegmentManager, WalSyncMode,
    WAL_MAGIC, WAL_VERSION,
};
