//! Deterministic in-process cluster simulator (Weeks 37–38).

#![allow(clippy::expect_used)]

use std::collections::{HashMap, VecDeque};

use crate::config::RaftConfig;
use crate::error::RaftError;
use crate::log::ConfChange;
use crate::node::MemNode;
use crate::rpc::RpcMessage;
use crate::storage::MemStorage;
use crate::types::{LogIndex, NodeId, Role};

/// In-process multi-node cluster (no real network).
pub struct Cluster {
    pub(crate) nodes: HashMap<NodeId, MemNode>,
    inbox: VecDeque<(NodeId, NodeId, RpcMessage)>,
}

impl Cluster {
    /// Spin up voters `1..=n`.
    ///
    /// # Errors
    ///
    /// Node open failures.
    pub fn new_voters(n: u64) -> Result<Self, RaftError> {
        let voters: Vec<NodeId> = (1..=n).map(NodeId).collect();
        let mut nodes = HashMap::new();
        for id in &voters {
            let cfg = RaftConfig::simulation(*id, voters.clone());
            let node = MemNode::open(cfg, MemStorage::new())?;
            nodes.insert(*id, node);
        }
        Ok(Self {
            nodes,
            inbox: VecDeque::new(),
        })
    }

    /// Run until queues drain or `max_rounds` reached.
    ///
    /// # Errors
    ///
    /// Simulation or storage errors.
    pub fn run_rounds(&mut self, max_rounds: usize) -> Result<(), RaftError> {
        for _ in 0..max_rounds {
            self.tick_all(25)?;
            for _ in 0..256 {
                if self.inbox.is_empty() {
                    break;
                }
                self.deliver_all()?;
            }
        }
        Ok(())
    }

    /// Advance every node's clock.
    pub fn tick_all(&mut self, ms: u64) -> Result<(), RaftError> {
        let ids: Vec<_> = self.nodes.keys().copied().collect();
        for id in ids {
            let sends = self.nodes.get_mut(&id).expect("node").tick_ms(ms)?;
            for (to, msg) in sends {
                self.inbox.push_back((id, to, msg));
            }
        }
        Ok(())
    }

    /// Deliver all queued RPCs (messages to removed nodes are dropped).
    pub fn deliver_all(&mut self) -> Result<(), RaftError> {
        while let Some((from, to, msg)) = self.inbox.pop_front() {
            if !self.nodes.contains_key(&to) {
                continue;
            }
            let (resp, sends) = self.nodes.get_mut(&to).expect("node").step(from, msg)?;
            if let Some(r) = resp {
                self.inbox.push_back((to, from, r));
            }
            for (peer, out) in sends {
                self.inbox.push_back((to, peer, out));
            }
        }
        Ok(())
    }

    /// Return id of current leader if exactly one.
    #[must_use]
    pub fn leader(&self) -> Option<NodeId> {
        let leaders: Vec<_> = self
            .nodes
            .iter()
            .filter(|(_, n)| n.raft().role() == Role::Leader)
            .map(|(id, _)| *id)
            .collect();
        if leaders.len() == 1 {
            Some(leaders[0])
        } else {
            None
        }
    }

    /// Total applied commands across all nodes.
    #[must_use]
    pub fn applied_count(&self) -> usize {
        self.nodes.values().map(|n| n.applied().len()).sum()
    }

    /// Drive ticks and delivery until the inbox is quiet or `max_rounds` elapses.
    ///
    /// Faster than [`Self::run_rounds`] for in-process replication (no extra deliver loop per tick).
    pub fn drive_quiescent(&mut self, max_rounds: usize) -> Result<(), RaftError> {
        for _ in 0..max_rounds {
            while !self.inbox.is_empty() {
                self.deliver_all()?;
            }
            self.tick_all(25)?;
        }
        Ok(())
    }

    /// Propose on the current leader and replicate.
    ///
    /// # Errors
    ///
    /// No leader or replication failure.
    pub fn propose_on_leader(&mut self, cmd: Vec<u8>) -> Result<(), RaftError> {
        let leader = self
            .leader()
            .ok_or_else(|| RaftError::internal("no leader"))?;
        let sends = self.nodes.get_mut(&leader).expect("leader").propose(cmd)?;
        for (to, msg) in sends {
            self.inbox.push_back((leader, to, msg));
        }
        self.drive_quiescent(16)?;
        match self.nodes.get(&leader) {
            Some(n) if !n.applied().is_empty() => {}
            _ => return Err(RaftError::internal("not replicated")),
        }
        Ok(())
    }

    /// Role of a given node.
    #[must_use]
    pub fn raft_role(&self, id: NodeId) -> Role {
        self.nodes.get(&id).expect("node").raft().role()
    }

    /// Voter ids in this cluster.
    #[must_use]
    pub fn voter_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Committed-applied command log for a node.
    #[must_use]
    pub fn applied_at(&self, id: NodeId) -> &[Vec<u8>] {
        self.nodes.get(&id).expect("node").applied()
    }

    /// Commit index for a node.
    #[must_use]
    pub fn commit_index(&self, id: NodeId) -> LogIndex {
        self.nodes.get(&id).expect("node").raft().commit_index()
    }

    /// Block until leader commits a linearizable read barrier (Week 26).
    ///
    /// # Errors
    ///
    /// No leader or barrier not committed in time.
    pub fn linearizable_barrier(&mut self) -> Result<LogIndex, RaftError> {
        let leader = self
            .leader()
            .ok_or_else(|| RaftError::internal("no leader"))?;
        let read_id = 1;
        let sends = self
            .nodes
            .get_mut(&leader)
            .expect("leader")
            .read_index(read_id)?;
        for (to, msg) in sends {
            self.inbox.push_back((leader, to, msg));
        }
        let target = self
            .nodes
            .get(&leader)
            .expect("leader")
            .raft()
            .log()
            .last_index();
        for _ in 0..48 {
            self.drive_quiescent(8)?;
            if self.commit_index(leader) >= target {
                return Ok(target);
            }
        }
        Err(RaftError::internal("read index barrier timeout"))
    }

    /// Add a voter via joint-conf change and replicate (Week 27).
    ///
    /// # Errors
    ///
    /// Cluster errors.
    pub fn add_voter(&mut self, id: NodeId) -> Result<(), RaftError> {
        if self.nodes.contains_key(&id) {
            return Ok(());
        }
        let voters: Vec<_> = self.nodes.keys().copied().collect();
        let cfg = RaftConfig::simulation(id, {
            let mut v = voters;
            if !v.contains(&id) {
                v.push(id);
            }
            v.sort_unstable();
            v
        });
        let node = MemNode::open(cfg, MemStorage::new())?;
        self.nodes.insert(id, node);
        self.run_rounds(8)?;
        let leader = self
            .leader()
            .ok_or_else(|| RaftError::internal("no leader"))?;
        let sends = self
            .nodes
            .get_mut(&leader)
            .expect("leader")
            .propose_conf_change(ConfChange::AddVoter(id))?;
        for (to, msg) in sends {
            self.inbox.push_back((leader, to, msg));
        }
        self.drive_quiescent(32)?;
        Ok(())
    }

    /// Voter count.
    #[must_use]
    pub fn voter_count(&self) -> usize {
        self.nodes.len()
    }

    /// Remove a node from the simulation (chaos / fault injection).
    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.remove(&id);
        self.inbox.retain(|(_, to, _)| *to != id);
    }
}
