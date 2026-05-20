//! LSM-tree storage engine for NoeDB.
//!
//! This crate will own the on-disk format and the in-memory MemTable
//! once Phase 2 of the sprint begins (Week 09). Today it only exists so
//! that the workspace dependency graph is complete and the planner can
//! reserve its place at the table.
//!
//! Planned modules:
//!
//! - `memtable`: in-memory sorted map, write-optimised.
//! - `wal`: append-only write-ahead log with `fsync` semantics.
//! - `sstable`: immutable on-disk sorted runs.
//! - `bloom`: per-SSTable Bloom filter.
//! - `compaction`: leveled / tiered compaction strategy (TBD Week 14).
//!
//! See `docs/sprint-plan.md`, Phase 2.

#![forbid(unsafe_code)]

/// Reserved: the central trait every storage backend will implement.
///
/// Defined in Week 09 once we have an AST and a planner to drive it.
pub trait StorageEngine {
    /// Storage-level error type.
    type Error;
}
