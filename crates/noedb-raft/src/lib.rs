//! Raft consensus implementation for NoeDB.
//!
//! Phase 4 of the sprint (Weeks 29-44) implements the Raft algorithm
//! described in:
//!
//! > Ongaro & Ousterhout (2014),
//! > *In Search of an Understandable Consensus Algorithm*.
//!
//! This crate will own leader election, log replication, membership
//! changes, and snapshotting. Today it is a placeholder so that the
//! workspace's dependency graph is complete and the planner / storage
//! crates can wire log entries through it later.
//!
//! See `docs/sprint-plan.md`, Phase 4.

#![forbid(unsafe_code)]

/// Reserved: the central state machine trait every Raft node will
/// implement.
pub trait StateMachine {
    /// State-machine-specific command type.
    type Command;
    /// Output of applying a command.
    type Output;
}
