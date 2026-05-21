//! Persistent and volatile Raft state (Week 34).

use serde::{Deserialize, Serialize};

use crate::types::{LogIndex, NodeId, Role, Term};

/// Durable metadata (must survive crashes before replying to RPCs).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardState {
    /// Latest term seen.
    pub current_term: Term,
    /// Candidate id voted for in `current_term`, if any.
    pub voted_for: Option<NodeId>,
    /// Highest log index known to be committed.
    pub commit_index: LogIndex,
    /// Highest log index applied to state machine.
    pub last_applied: LogIndex,
}

/// Ephemeral role and leader progress.
#[derive(Debug, Clone, Default)]
pub struct SoftState {
    /// Current role.
    pub role: Role,
    /// Known leader id (if any).
    pub leader_id: Option<NodeId>,
}

/// Per-follower replication state on the leader.
#[derive(Debug, Clone, Copy, Default)]
pub struct Progress {
    /// Next log entry to send.
    pub next_index: LogIndex,
    /// Highest index known replicated on follower.
    pub match_index: LogIndex,
}
