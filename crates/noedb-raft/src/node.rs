//! Raft node runtime — applies [`Action`]s to storage (Week 37).

#![allow(clippy::type_complexity)]

use crate::config::RaftConfig;
use crate::error::RaftError;
use crate::raft_core::{Action, Raft};
use crate::rpc::RpcMessage;
use crate::storage::{MemStorage, RaftStorage};
use crate::types::NodeId;

/// One Raft peer with durable storage.
pub struct RaftNode<S: RaftStorage> {
    raft: Raft,
    storage: S,
    applied: Vec<Vec<u8>>,
}

impl<S: RaftStorage> RaftNode<S> {
    /// Open node: load durable state then build Raft core.
    ///
    /// # Errors
    ///
    /// Storage errors on load.
    pub fn open(config: RaftConfig, storage: S) -> Result<Self, RaftError> {
        let hard = storage.load_hard_state()?;
        let log = storage.load_log()?;
        Ok(Self {
            raft: Raft::new(config, hard, log),
            storage,
            applied: Vec::new(),
        })
    }

    /// Access inner Raft (tests / simulation).
    #[must_use]
    pub const fn raft(&self) -> &Raft {
        &self.raft
    }

    /// Mutable Raft core.
    pub const fn raft_mut(&mut self) -> &mut Raft {
        &mut self.raft
    }

    /// Commands applied locally.
    #[must_use]
    pub fn applied(&self) -> &[Vec<u8>] {
        &self.applied
    }

    /// Advance timers; returns outbound RPCs.
    ///
    /// # Errors
    ///
    /// Storage errors while persisting.
    pub fn tick_ms(&mut self, ms: u64) -> Result<Vec<(NodeId, RpcMessage)>, RaftError> {
        let actions = self.raft.tick(ms);
        self.exec(actions)
    }

    /// Handle inbound RPC.
    ///
    /// # Errors
    ///
    /// Storage errors while persisting.
    pub fn step(
        &mut self,
        from: NodeId,
        msg: RpcMessage,
    ) -> Result<(Option<RpcMessage>, Vec<(NodeId, RpcMessage)>), RaftError> {
        let (resp, actions) = self.dispatch(from, msg);
        let outbound = self.exec(actions)?;
        Ok((resp, outbound))
    }

    /// Propose a write (leader only).
    ///
    /// # Errors
    ///
    /// Not leader or storage failure.
    pub fn propose(&mut self, cmd: Vec<u8>) -> Result<Vec<(NodeId, RpcMessage)>, RaftError> {
        let actions = self.raft.propose(cmd)?;
        self.exec(actions)
    }

    /// Linearizable read barrier (leader only).
    ///
    /// # Errors
    ///
    /// Not leader or storage failure.
    pub fn read_index(&mut self, read_id: u64) -> Result<Vec<(NodeId, RpcMessage)>, RaftError> {
        let actions = self.raft.read_index(read_id)?;
        self.exec(actions)
    }

    /// Propose membership change (leader only).
    pub fn propose_conf_change(
        &mut self,
        cc: crate::log::ConfChange,
    ) -> Result<Vec<(NodeId, RpcMessage)>, RaftError> {
        let actions = self.raft.propose_conf_change(cc)?;
        self.exec(actions)
    }

    fn dispatch(&mut self, from: NodeId, msg: RpcMessage) -> (Option<RpcMessage>, Vec<Action>) {
        match msg {
            RpcMessage::RequestVoteResp(resp) => (None, self.raft.step_vote_response(from, &resp)),
            RpcMessage::AppendEntriesResp(resp) => (None, self.raft.step_append_response(from, &resp)),
            other => self.raft.step(from, other),
        }
    }

    fn exec(&mut self, actions: Vec<Action>) -> Result<Vec<(NodeId, RpcMessage)>, RaftError> {
        let mut outbound = Vec::new();
        for action in actions {
            match action {
                Action::PersistHardState => {
                    self.storage.save_hard_state(self.raft.hard_state())?;
                }
                Action::PersistLog { from, entries } => {
                    self.storage.append(from, &entries)?;
                }
                Action::Apply(cmds) => {
                    self.applied.extend(cmds);
                }
                Action::Snapshot(snap) => {
                    self.storage.save_snapshot(&snap)?;
                    self.raft
                        .log_mut()
                        .restore(snap.index, snap.term, Vec::new());
                }
                Action::Send { to, msg } => outbound.push((to, msg)),
            }
        }
        self.storage.sync()?;
        Ok(outbound)
    }
}

/// In-memory node (tests).
pub type MemNode = RaftNode<MemStorage>;
