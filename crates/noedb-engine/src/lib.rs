//! End-to-end SQL engine for NoeDB (Phase 5).
//!
//! Wires **parser → planner → Raft → LSM** with bounded inputs and
//! deterministic in-process clustering for integration tests.

#![forbid(unsafe_code)]

mod command;
mod engine;
mod error;
mod machine;

pub use command::{Command, MAX_COMMAND_BYTES};
pub use engine::{
    validate_sql, DistributedEngine, LocalEngine, QueryResult, MAX_SQL_BYTES,
};
pub use error::EngineError;
pub use machine::{apply_command, row_key};
