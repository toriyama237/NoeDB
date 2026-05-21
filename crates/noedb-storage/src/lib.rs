//! LSM-tree storage engine for NoeDB.
//!
//! Phase 2 builds the on-disk persistence stack piece by piece:
//!
//! - **Week 09** — [`MemTable`]: in-memory sorted map.
//! - **Week 10** — [`Wal`] + [`DurableStore`]: durable WAL-first writes,
//!   MemTable rotation when `max_mem_bytes` is reached.
//! - **Week 11** — WAL replay on startup + crash recovery.
//! - **Week 12–16** — SSTable, Bloom filter, compaction, full LSM.
//!
//! See `docs/sprint-plan.md`, Phase 2.

#![forbid(unsafe_code)]
#![allow(
    clippy::missing_const_for_fn,
    clippy::use_self,
    clippy::explicit_iter_loop,
    clippy::cast_possible_truncation
)]

mod checksum;
mod durable;
mod engine;
mod error;
mod memtable;
mod wal;

pub use crate::durable::DurableStore;
pub use crate::engine::StorageEngine;
pub use crate::error::StorageError;
pub use crate::memtable::{
    MemTable, MemTableIter, MemTableRangeIter, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_MEM_BYTES,
};
pub use crate::wal::{replay_into_memtable, LogEntry, OpType, Wal, WAL_MAGIC, WAL_VERSION};
