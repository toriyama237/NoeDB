//! Raft timing and cluster configuration.

#![allow(clippy::cast_possible_truncation)]
//!
//! Follows the Raft inequality: `broadcastTime ≪ electionTimeout ≪ MTBF`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::types::NodeId;

/// Tunable Raft parameters (production-oriented defaults).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftConfig {
    /// This node's id.
    pub id: NodeId,
    /// All voter ids (including self).
    pub voters: Vec<NodeId>,
    /// Minimum election timeout.
    pub election_timeout_min: Duration,
    /// Maximum election timeout (randomized in `[min, max]`).
    pub election_timeout_max: Duration,
    /// Leader heartbeat interval.
    pub heartbeat_interval: Duration,
    /// Max entries per `AppendEntries` RPC (batching).
    pub max_append_batch: usize,
    /// Max in-flight bytes per follower pipeline.
    pub max_pipeline_bytes: usize,
    /// Cluster auth token.
    pub auth: crate::security::ClusterAuth,
}

impl RaftConfig {
    /// Three-node LAN-oriented defaults.
    #[must_use]
    pub fn three_node(id: NodeId) -> Self {
        Self {
            id,
            voters: vec![NodeId(1), NodeId(2), NodeId(3)],
            election_timeout_min: Duration::from_millis(300),
            election_timeout_max: Duration::from_millis(500),
            heartbeat_interval: Duration::from_millis(50),
            max_append_batch: 256,
            max_pipeline_bytes: 512 * 1024,
            auth: crate::security::ClusterAuth::from_passphrase("noedb-dev-cluster"),
        }
    }

    /// Fast deterministic settings for simulation tests (no random election delay).
    #[must_use]
    pub fn simulation(id: NodeId, voters: Vec<NodeId>) -> Self {
        let stagger = 80 + id.0 * 40;
        Self {
            id,
            voters,
            election_timeout_min: Duration::from_millis(stagger),
            election_timeout_max: Duration::from_millis(stagger),
            heartbeat_interval: Duration::from_millis(25),
            max_append_batch: 64,
            max_pipeline_bytes: 256 * 1024,
            auth: crate::security::ClusterAuth::from_passphrase("noedb-sim-cluster"),
        }
    }

    /// Majority quorum size.
    #[must_use]
    pub fn quorum(&self) -> usize {
        self.voters.len() / 2 + 1
    }

    /// Whether `node` is a voting member.
    #[must_use]
    pub fn is_voter(&self, node: NodeId) -> bool {
        self.voters.contains(&node)
    }

    /// Election timeout for this node (deterministic when min == max).
    #[must_use]
    pub fn election_timeout(&self) -> Duration {
        use rand::Rng;
        let min = self.election_timeout_min.as_millis() as u64;
        let max = self.election_timeout_max.as_millis() as u64;
        if min == max {
            return Duration::from_millis(min);
        }
        let max = max.max(min);
        let ms = rand::thread_rng().gen_range(min..=max);
        Duration::from_millis(ms)
    }
}
