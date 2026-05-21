//! Replicated log (Week 33–34).

#![allow(clippy::cast_possible_truncation)]

use serde::{Deserialize, Serialize};

use crate::types::{LogIndex, Term};

/// User command replicated through Raft (opaque bytes for the state machine).
pub type Command = Vec<u8>;

/// One Raft log entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    /// Term when entry was created by leader.
    pub term: Term,
    /// Command payload for the state machine.
    pub command: Command,
}

/// Membership change (Week 39 — single-step conf change).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(dead_code)]
pub enum ConfChange {
    /// Add a voter.
    AddVoter(crate::types::NodeId),
    /// Remove a voter.
    RemoveVoter(crate::types::NodeId),
}

/// In-memory replicated log with sentinel entry at index 0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftLog {
    /// `entries[0]` is a sentinel; real entries start at index 1.
    entries: Vec<LogEntry>,
}

impl Default for RaftLog {
    fn default() -> Self {
        Self::new()
    }
}

impl RaftLog {
    /// Empty log with sentinel.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: vec![LogEntry {
                term: Term(0),
                command: Vec::new(),
            }],
        }
    }

    /// Last log index.
    #[must_use]
    pub fn last_index(&self) -> LogIndex {
        LogIndex(self.entries.len().saturating_sub(1) as u64)
    }

    /// Term of entry at `index` (0 if missing).
    #[must_use]
    pub fn term_at(&self, index: LogIndex) -> Term {
        self.entries
            .get(index.0 as usize)
            .map_or(Term(0), |e| e.term)
    }

    /// Entry at `index`.
    #[must_use]
    pub fn get(&self, index: LogIndex) -> Option<&LogEntry> {
        self.entries.get(index.0 as usize)
    }

    /// Append entries; returns first new index.
    pub fn append(&mut self, entries: &[LogEntry]) -> LogIndex {
        let start = self.last_index().next();
        self.entries.extend_from_slice(entries);
        start
    }

    /// Truncate log after `index` (keep `0..=index`).
    pub fn truncate_from(&mut self, index: LogIndex) {
        let keep = (index.0 as usize).saturating_add(1);
        if keep < self.entries.len() {
            self.entries.truncate(keep);
        }
    }

    /// Slice `(from..]` for replication.
    #[must_use]
    pub fn slice_from(&self, from: LogIndex) -> &[LogEntry] {
        let start = from.0 as usize;
        if start >= self.entries.len() {
            &[]
        } else {
            &self.entries[start..]
        }
    }

    /// Full entry list (including sentinel).
    #[must_use]
    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Replace log from snapshot + suffix.
    pub fn restore(&mut self, snapshot_index: LogIndex, snapshot_term: Term, suffix: Vec<LogEntry>) {
        self.entries.clear();
        self.entries.push(LogEntry {
            term: snapshot_term,
            command: Vec::new(),
        });
        if snapshot_index.0 > 0 {
            self.entries.resize(snapshot_index.0 as usize + 1, LogEntry {
                term: snapshot_term,
                command: Vec::new(),
            });
            if let Some(last) = self.entries.last_mut() {
                last.term = snapshot_term;
            }
        }
        self.entries.extend(suffix);
    }
}
