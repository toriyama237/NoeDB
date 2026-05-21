//! Raft RPC messages (Weeks 32–33, 40, 41).

use serde::{Deserialize, Serialize};

use crate::log::LogEntry;
use crate::types::{LogIndex, NodeId, Term};

/// Top-level RPC message enum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RpcMessage {
    /// Candidate requests votes.
    RequestVote(RequestVoteReq),
    /// Vote response.
    RequestVoteResp(RequestVoteResp),
    /// Leader replicates log / heartbeats.
    AppendEntries(AppendEntriesReq),
    /// Replication response.
    AppendEntriesResp(AppendEntriesResp),
    /// Leader ships a snapshot.
    InstallSnapshot(InstallSnapshotReq),
    /// Snapshot response.
    InstallSnapshotResp(InstallSnapshotResp),
    /// Linearizable read (Week 41).
    ReadIndex(ReadIndexReq),
    /// Read index confirmation.
    ReadIndexResp(ReadIndexResp),
}

/// `RequestVote` RPC (§5.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestVoteReq {
    /// Candidate's term.
    pub term: Term,
    /// Candidate id.
    pub candidate_id: NodeId,
    /// Index of candidate's last log entry.
    pub last_log_index: LogIndex,
    /// Term of candidate's last log entry.
    pub last_log_term: Term,
}

/// `RequestVote` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestVoteResp {
    /// Responder's current term.
    pub term: Term,
    /// Whether vote was granted.
    pub vote_granted: bool,
}

/// `AppendEntries` RPC (§5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEntriesReq {
    /// Leader's term.
    pub term: Term,
    /// Leader id.
    pub leader_id: NodeId,
    /// Index of log entry immediately preceding new ones.
    pub prev_log_index: LogIndex,
    /// Term of `prev_log_index` entry.
    pub prev_log_term: Term,
    /// Log entries to store (empty for heartbeat).
    pub entries: Vec<LogEntry>,
    /// Leader's `commit_index`.
    pub leader_commit: LogIndex,
}

/// `AppendEntries` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendEntriesResp {
    /// Responder's current term.
    pub term: Term,
    /// True if follower contained entry matching `prev_log_index` and `prev_log_term`.
    pub success: bool,
    /// Follower's last log index (for leader back-off).
    pub last_log_index: LogIndex,
}

/// `InstallSnapshot` RPC (§7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotReq {
    /// Leader term.
    pub term: Term,
    /// Leader id.
    pub leader_id: NodeId,
    /// Snapshot replaces all entries up through this index.
    pub last_included_index: LogIndex,
    /// Term of `last_included_index`.
    pub last_included_term: Term,
    /// Snapshot bytes (opaque to Raft).
    pub data: Vec<u8>,
    /// Offset for chunked transfer (0 = first chunk).
    pub offset: u64,
    /// True if this is the last chunk.
    pub done: bool,
}

/// `InstallSnapshot` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallSnapshotResp {
    /// Current term.
    pub term: Term,
}

/// `ReadIndex` request for linearizable reads (§6.4 / etcd-style).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadIndexReq {
    /// Leader term.
    pub term: Term,
    /// Leader id.
    pub leader_id: NodeId,
    /// Client correlation id.
    pub read_id: u64,
}

/// `ReadIndex` response once `commit_index >= read_index`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadIndexResp {
    /// Current term.
    pub term: Term,
    /// Safe read index.
    pub read_index: LogIndex,
    /// Echo client id.
    pub read_id: u64,
}
