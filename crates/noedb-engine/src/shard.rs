//! Hash sharding router (Phase 4 Weeks 29–32).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Routes row keys to shard ids for horizontal scale-out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardRouter {
    shards: usize,
}

impl ShardRouter {
    /// New router with `shards` partitions (minimum 1).
    #[must_use]
    pub fn new(shards: usize) -> Self {
        Self {
            shards: shards.max(1),
        }
    }

    /// Number of shards.
    #[must_use]
    pub const fn shard_count(&self) -> usize {
        self.shards
    }

    /// Shard id for `(table, row)`.
    #[must_use]
    pub fn route(&self, table: &str, row: &str) -> usize {
        let mut h = DefaultHasher::new();
        table.hash(&mut h);
        row.hash(&mut h);
        (h.finish() as usize) % self.shards
    }
}
