//! Core Raft state machine (Weeks 31–33) — pure, deterministic, testable.

#![allow(clippy::cast_possible_truncation, clippy::needless_pass_by_ref_mut)]

use std::collections::{BTreeMap, HashSet};

use crate::error::RaftError;
use crate::config::RaftConfig;
use crate::log::{LogEntry, RaftLog};
use crate::rpc::{
    AppendEntriesReq, AppendEntriesResp, InstallSnapshotReq, InstallSnapshotResp, ReadIndexReq,
    ReadIndexResp, RequestVoteReq, RequestVoteResp, RpcMessage,
};
use crate::state::{HardState, Progress, SoftState};
use crate::storage::Snapshot;
use crate::types::{LogIndex, NodeId, Role, Term};

/// Side effects for the runtime to execute (I/O, network).
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub enum Action {
    /// Persist hard state before responding.
    PersistHardState,
    /// Persist log suffix.
    PersistLog {
        /// Truncate after this index before append.
        from: LogIndex,
        /// New entries.
        entries: Vec<LogEntry>,
    },
    /// Send RPC to peer.
    Send {
        /// Target node.
        to: NodeId,
        /// Message.
        msg: RpcMessage,
    },
    /// Apply committed commands to state machine.
    Apply(Vec<Vec<u8>>),
}

/// In-memory Raft node (drive with [`Raft::tick`] / [`Raft::step`]).
pub struct Raft {
    config: RaftConfig,
    hard: HardState,
    soft: SoftState,
    log: RaftLog,
    /// Leader replication progress.
    progress: BTreeMap<NodeId, Progress>,
    /// Votes received this election.
    votes: HashSet<NodeId>,
    /// Milliseconds until election fires (reset on valid AppendEntries).
    election_remaining_ms: u64,
    /// Leader heartbeat accumulator.
    heartbeat_remaining_ms: u64,
    /// Pending linearizable reads (leader).
    pending_reads: Vec<(u64, LogIndex)>,
}

impl Raft {
    /// Create a node from config + recovered state.
    #[must_use]
    pub fn new(config: RaftConfig, hard: HardState, log: RaftLog) -> Self {
        let election_remaining_ms = config.election_timeout().as_millis() as u64;
        Self {
            config,
            hard,
            soft: SoftState::default(),
            log,
            progress: BTreeMap::new(),
            votes: HashSet::new(),
            election_remaining_ms,
            heartbeat_remaining_ms: 0,
            pending_reads: Vec::new(),
        }
    }

    /// Current role.
    #[must_use]
    pub const fn role(&self) -> Role {
        self.soft.role
    }

    /// Current term.
    #[must_use]
    pub const fn term(&self) -> Term {
        self.hard.current_term
    }

    /// Immutable view of hard state.
    #[must_use]
    pub const fn hard_state(&self) -> &HardState {
        &self.hard
    }

    /// Shared log reference.
    #[must_use]
    pub const fn log(&self) -> &RaftLog {
        &self.log
    }

    /// Advance timers by `elapsed_ms`; returns actions to run.
    pub fn tick(&mut self, elapsed_ms: u64) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.soft.role == Role::Leader {
            if self.heartbeat_remaining_ms <= elapsed_ms {
                self.heartbeat_remaining_ms = self.config.heartbeat_interval.as_millis() as u64;
                actions.extend(self.broadcast_append());
            } else {
                self.heartbeat_remaining_ms -= elapsed_ms;
            }
            return actions;
        }

        if self.election_remaining_ms <= elapsed_ms {
            actions.extend(self.start_election());
        } else {
            self.election_remaining_ms -= elapsed_ms;
        }
        actions
    }

    /// Handle an inbound RPC from `from`.
    pub fn step(&mut self, from: NodeId, msg: RpcMessage) -> (Option<RpcMessage>, Vec<Action>) {
        match msg {
            RpcMessage::RequestVote(req) => self.handle_request_vote(from, &req),
            RpcMessage::AppendEntries(req) => self.handle_append_entries(from, &req),
            RpcMessage::InstallSnapshot(req) => self.handle_install_snapshot(from, &req),
            RpcMessage::ReadIndex(req) => self.handle_read_index(from, &req),
            other => (
                None,
                vec![Action::Send {
                    to: from,
                    msg: other,
                }],
            ),
        }
    }

    /// Propose a command (must run on leader).
    ///
    /// # Errors
    ///
    /// Returns error string if not leader.
    pub fn propose(&mut self, cmd: Vec<u8>) -> Result<Vec<Action>, RaftError> {
        if self.soft.role != Role::Leader {
            return Err(RaftError::internal("not leader"));
        }
        let entry = LogEntry {
            term: self.hard.current_term,
            command: cmd,
        };
        let from = self.log.last_index();
        self.log.append(std::slice::from_ref(&entry));
        let mut actions = vec![Action::PersistLog {
            from: from.next(),
            entries: vec![entry],
        }];
        actions.extend(self.broadcast_append());
        Ok(actions)
    }

    fn reset_election_timer(&mut self) {
        self.election_remaining_ms = self.config.election_timeout().as_millis() as u64;
    }

    fn become_follower(&mut self, term: Term, leader: Option<NodeId>) -> Vec<Action> {
        self.soft.role = Role::Follower;
        self.soft.leader_id = leader;
        self.votes.clear();
        self.progress.clear();
        self.pending_reads.clear();
        if term > self.hard.current_term {
            self.hard.current_term = term;
            self.hard.voted_for = None;
        }
        self.reset_election_timer();
        vec![Action::PersistHardState]
    }

    fn become_candidate(&mut self) -> Vec<Action> {
        let term = self.hard.current_term.next();
        self.hard.current_term = term;
        self.hard.voted_for = Some(self.config.id);
        self.soft.role = Role::Candidate;
        self.soft.leader_id = None;
        self.votes.clear();
        self.votes.insert(self.config.id);
        self.reset_election_timer();

        let last = self.log.last_index();
        let last_term = self.log.term_at(last);
        let mut actions = vec![Action::PersistHardState];
        for peer in &self.config.voters {
            if *peer == self.config.id {
                continue;
            }
            actions.push(Action::Send {
                to: *peer,
                msg: RpcMessage::RequestVote(RequestVoteReq {
                    term,
                    candidate_id: self.config.id,
                    last_log_index: last,
                    last_log_term: last_term,
                }),
            });
        }
        if self.votes.len() >= self.config.quorum() {
            actions.extend(self.become_leader());
        }
        actions
    }

    fn become_leader(&mut self) -> Vec<Action> {
        self.soft.role = Role::Leader;
        self.soft.leader_id = Some(self.config.id);
        self.progress.clear();
        let next = self.log.last_index().next();
        for peer in &self.config.voters {
            if *peer != self.config.id {
                self.progress.insert(
                    *peer,
                    Progress {
                        next_index: next,
                        match_index: LogIndex(0),
                    },
                );
            }
        }
        self.heartbeat_remaining_ms = 0;
        self.broadcast_append()
    }

    fn start_election(&mut self) -> Vec<Action> {
        if self.soft.role == Role::Leader {
            return Vec::new();
        }
        self.become_candidate()
    }

    fn handle_request_vote(&mut self, _from: NodeId, req: &RequestVoteReq) -> (Option<RpcMessage>, Vec<Action>) {
        let mut actions = Vec::new();
        if req.term < self.hard.current_term {
            return (
                Some(RpcMessage::RequestVoteResp(RequestVoteResp {
                    term: self.hard.current_term,
                    vote_granted: false,
                })),
                actions,
            );
        }
        if req.term > self.hard.current_term {
            actions.extend(self.become_follower(req.term, None));
        }
        if (self.hard.voted_for.is_none() || self.hard.voted_for == Some(req.candidate_id))
            && self.log_is_up_to_date(req.last_log_index, req.last_log_term)
        {
            self.hard.voted_for = Some(req.candidate_id);
            self.reset_election_timer();
            actions.push(Action::PersistHardState);
            (
                Some(RpcMessage::RequestVoteResp(RequestVoteResp {
                    term: self.hard.current_term,
                    vote_granted: true,
                })),
                actions,
            )
        } else {
            (
                Some(RpcMessage::RequestVoteResp(RequestVoteResp {
                    term: self.hard.current_term,
                    vote_granted: false,
                })),
                actions,
            )
        }
    }

    fn handle_append_entries(
        &mut self,
        _from: NodeId,
        req: &AppendEntriesReq,
    ) -> (Option<RpcMessage>, Vec<Action>) {
        let mut actions = Vec::new();
        if req.term < self.hard.current_term {
            return (
                Some(RpcMessage::AppendEntriesResp(AppendEntriesResp {
                    term: self.hard.current_term,
                    success: false,
                    last_log_index: self.log.last_index(),
                })),
                actions,
            );
        }
        if req.term > self.hard.current_term
            || (req.term == self.hard.current_term && self.soft.role == Role::Leader && req.leader_id != self.config.id)
        {
            actions.extend(self.become_follower(req.term, Some(req.leader_id)));
        } else {
            self.soft.leader_id = Some(req.leader_id);
            self.reset_election_timer();
        }

        let ok = self.log.term_at(req.prev_log_index) == req.prev_log_term;
        if !ok {
            return (
                Some(RpcMessage::AppendEntriesResp(AppendEntriesResp {
                    term: self.hard.current_term,
                    success: false,
                    last_log_index: self.log.last_index(),
                })),
                actions,
            );
        }

        if !req.entries.is_empty() {
            let from_idx = req.prev_log_index.next();
            self.log.truncate_from(req.prev_log_index);
            self.log.append(&req.entries);
            actions.push(Action::PersistLog {
                from: from_idx,
                entries: req.entries.clone(),
            });
        }

        if req.leader_commit > self.hard.commit_index {
            let last_new = self.log.last_index();
            let commit = req.leader_commit.min(last_new);
            if self.log.term_at(commit) == req.term {
                self.hard.commit_index = commit;
                actions.push(Action::PersistHardState);
                actions.extend(self.apply_committed());
            }
        }

        (
            Some(RpcMessage::AppendEntriesResp(AppendEntriesResp {
                term: self.hard.current_term,
                success: true,
                last_log_index: self.log.last_index(),
            })),
            actions,
        )
    }

    fn handle_install_snapshot(
        &mut self,
        _from: NodeId,
        req: &InstallSnapshotReq,
    ) -> (Option<RpcMessage>, Vec<Action>) {
        let mut actions = Vec::new();
        if req.term >= self.hard.current_term {
            actions.extend(self.become_follower(req.term, Some(req.leader_id)));
            self.log.restore(req.last_included_index, req.last_included_term, Vec::new());
            if req.done {
                let snap = Snapshot {
                    index: req.last_included_index,
                    term: req.last_included_term,
                    data: req.data.clone(),
                };
                let _ = snap;
                self.hard.commit_index = req.last_included_index;
                actions.push(Action::PersistHardState);
            }
        }
        (
            Some(RpcMessage::InstallSnapshotResp(InstallSnapshotResp {
                term: self.hard.current_term,
            })),
            actions,
        )
    }

    fn handle_read_index(&mut self, from: NodeId, req: &ReadIndexReq) -> (Option<RpcMessage>, Vec<Action>) {
        if self.soft.role != Role::Leader || req.term != self.hard.current_term {
            return (None, Vec::new());
        }
        let read_index = self.log.last_index();
        self.pending_reads.push((req.read_id, read_index));
        let mut actions = self.broadcast_append();
        actions.extend(self.try_confirm_reads());
        let _ = from;
        (
            Some(RpcMessage::ReadIndexResp(ReadIndexResp {
                term: self.hard.current_term,
                read_index,
                read_id: req.read_id,
            })),
            actions,
        )
    }

    /// Process vote response (candidate only).
    pub fn step_vote_response(&mut self, from: NodeId, resp: &RequestVoteResp) -> Vec<Action> {
        if resp.term > self.hard.current_term {
            return self.become_follower(resp.term, None);
        }
        if self.soft.role != Role::Candidate || resp.term != self.hard.current_term {
            return Vec::new();
        }
        if resp.vote_granted {
            self.votes.insert(from);
        }
        if self.votes.len() >= self.config.quorum() {
            return self.become_leader();
        }
        Vec::new()
    }

    /// Process append response (leader only).
    pub fn step_append_response(&mut self, from: NodeId, resp: &AppendEntriesResp) -> Vec<Action> {
        if resp.term > self.hard.current_term {
            return self.become_follower(resp.term, None);
        }
        if self.soft.role != Role::Leader {
            return Vec::new();
        }
        let Some(prog) = self.progress.get_mut(&from) else {
            return Vec::new();
        };
        if resp.success {
            prog.match_index = resp.last_log_index;
            prog.next_index = resp.last_log_index.next();
        } else {
            prog.next_index = LogIndex(prog.next_index.0.saturating_sub(1).max(1));
        }
        let mut actions = self.try_advance_commit();
        let next = self.progress.get(&from).map_or(LogIndex(0), |p| p.next_index);
        if next <= self.log.last_index() {
            actions.extend(self.append_to_peer(from));
        }
        actions
    }

    fn try_advance_commit(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let current_term = self.hard.current_term;
        for index in (self.hard.commit_index.0 + 1)..=self.log.last_index().0 {
            let idx = LogIndex(index);
            if self.log.term_at(idx) != current_term {
                continue;
            }
            let mut count = 1usize;
            for peer in &self.config.voters {
                if *peer == self.config.id {
                    continue;
                }
                if let Some(p) = self.progress.get(peer) {
                    if p.match_index >= idx {
                        count += 1;
                    }
                }
            }
            if count >= self.config.quorum() {
                self.hard.commit_index = idx;
                actions.push(Action::PersistHardState);
            }
        }
        actions.extend(self.apply_committed());
        actions.extend(self.try_confirm_reads());
        actions
    }

    fn try_confirm_reads(&mut self) -> Vec<Action> {
        self.pending_reads
            .retain(|(_, idx)| *idx > self.hard.commit_index);
        Vec::new()
    }

    fn apply_committed(&mut self) -> Vec<Action> {
        let mut cmds = Vec::new();
        while self.hard.last_applied < self.hard.commit_index {
            let next = self.hard.last_applied.next();
            if let Some(e) = self.log.get(next) {
                if !e.command.is_empty() {
                    cmds.push(e.command.clone());
                }
            }
            self.hard.last_applied = next;
        }
        if cmds.is_empty() {
            Vec::new()
        } else {
            vec![Action::Apply(cmds), Action::PersistHardState]
        }
    }

    fn broadcast_append(&mut self) -> Vec<Action> {
        if self.soft.role != Role::Leader {
            return Vec::new();
        }
        let peers: Vec<NodeId> = self
            .config
            .voters
            .iter()
            .copied()
            .filter(|p| *p != self.config.id)
            .collect();
        let mut actions = Vec::new();
        for peer in peers {
            actions.extend(self.append_to_peer(peer));
        }
        actions
    }

    fn append_to_peer(&mut self, peer: NodeId) -> Vec<Action> {
        if self.soft.role != Role::Leader {
            return Vec::new();
        }
        let next = self
            .progress
            .get(&peer)
            .map_or(LogIndex(1), |p| p.next_index);
        let prev = next.prev();
        let entries: Vec<_> = self
            .log
            .slice_from(next)
            .iter()
            .take(self.config.max_append_batch)
            .cloned()
            .collect();
        vec![Action::Send {
            to: peer,
            msg: RpcMessage::AppendEntries(AppendEntriesReq {
                term: self.hard.current_term,
                leader_id: self.config.id,
                prev_log_index: prev,
                prev_log_term: self.log.term_at(prev),
                entries,
                leader_commit: self.hard.commit_index,
            }),
        }]
    }

    fn log_is_up_to_date(&self, last_index: LogIndex, last_term: Term) -> bool {
        let my_last = self.log.last_index();
        let my_term = self.log.term_at(my_last);
        last_term > my_term || (last_term == my_term && last_index >= my_last)
    }
}
