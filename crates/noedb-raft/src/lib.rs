//! Raft consensus for NoeDB (Phase 4, Weeks 29–44).
//!
//! Implementation of the Raft algorithm (Ongaro & Ousterhout, 2014) with:
//!
//! - **Performance**: batched `AppendEntries`, pipelined replication, group-commit storage.
//! - **Robustness**: pure deterministic core FSM + property tests via [`sim::Cluster`].
//! - **Security**: bounded frames, cluster auth tokens, CRC-32 on durable records.
//!
//! # Architecture
//!
//! ```text
//!   RaftNode ──▶ Raft (raft_core) ──▶ Action (persist / send / apply)
//!        │              │
//!        ▼              ▼
//!   RaftStorage    Transport (TCP / memory)
//! ```

#![forbid(unsafe_code)]
#![allow(unreachable_pub)]

mod codec;
mod config;
mod error;
mod log;
mod membership;
mod node;
mod raft_core;
mod rpc;
mod security;
mod sim;
mod state;
mod storage;
mod transport;
mod types;

pub use codec::{decode_message, encode_message};
pub use config::RaftConfig;
pub use error::{RaftError, SecurityError, StorageError};
pub use log::{Command, ConfChange, LogEntry, RaftLog};
pub use membership::{decode_conf_change, encode_conf_change, JointConfig};
pub use node::{MemNode, RaftNode};
pub use raft_core::{Action, Raft};
pub use rpc::{
    AppendEntriesReq, AppendEntriesResp, InstallSnapshotReq, InstallSnapshotResp, ReadIndexReq,
    ReadIndexResp, RequestVoteReq, RequestVoteResp, RpcMessage,
};
pub use security::{checksum, ClusterAuth, WireEnvelope, MAX_FRAME_BYTES, WIRE_MAGIC};
pub use sim::Cluster;
pub use state::{HardState, Progress, SoftState};
pub use storage::{FileStorage, MemStorage, RaftStorage, Snapshot};
pub use transport::{MemoryTransport, TcpTransport, TlsTcpTransport, Transport};
pub use types::{LogIndex, NodeId, Role, Term};

/// State machine applied after commit (Week 45 integration).
pub trait StateMachine: Send {
    /// Command type.
    type Command;
    /// Apply output.
    type Output;
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn elects_leader_in_three_node_cluster() {
        let mut c = Cluster::new_voters(3).unwrap();
        c.run_rounds(80).unwrap();
        assert!(c.leader().is_some());
    }

    #[test]
    fn replicates_command() {
        let mut c = Cluster::new_voters(3).unwrap();
        c.run_rounds(80).unwrap();
        c.propose_on_leader(b"SET x 1".to_vec()).unwrap();
        assert!(c.applied_count() >= 1);
    }
}
