//! Core Raft identifiers and roles.

use serde::{Deserialize, Serialize};

/// Cluster member identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub u64);

/// Monotonic election term.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct Term(pub u64);

/// Index into the replicated log (1-based, index 0 is a sentinel empty entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub struct LogIndex(pub u64);

/// Node role in the Raft state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Role {
    /// Passive replica; votes and accepts entries from leader.
    #[default]
    Follower,
    /// Seeking votes to become leader.
    Candidate,
    /// Active leader; replicates log and advances commit index.
    Leader,
}

impl LogIndex {
    /// Previous index (saturating at 0).
    #[must_use]
    pub const fn prev(self) -> Self {
        Self(self.0.saturating_sub(1))
    }

    /// Next index.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl Term {
    /// Increment term (used when starting an election).
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}
