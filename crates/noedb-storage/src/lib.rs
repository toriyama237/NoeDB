//! LSM-tree storage engine for NoeDB.
//!
//! Phase 2 builds the on-disk persistence stack piece by piece:
//!
//! - **Week 09** — [`MemTable`]: in-memory sorted map (this crate today).
//! - **Week 10–11** — WAL + flush + crash recovery.
//! - **Week 12–13** — SSTable writer/reader.
//! - **Week 14–16** — Bloom filter, compaction, full [`LsmTree`].
//!
//! See `docs/sprint-plan.md`, Phase 2.
//!
//! [`LsmTree`]: memtable::MemTable

#![forbid(unsafe_code)]

mod engine;
mod error;
mod memtable;

pub use crate::engine::StorageEngine;
pub use crate::error::StorageError;
pub use crate::memtable::{MemTable, MemTableIter, MemTableRangeIter, DEFAULT_MAX_ENTRIES};
