//! End-to-end SQL engine for NoeDB (Phase 5).
//!
//! Wires **parser → planner → Raft → LSM** with bounded inputs and
//! deterministic in-process clustering for integration tests.

#![forbid(unsafe_code)]

mod audit;
mod command;
mod engine;
mod error;
mod machine;
mod prepared;
mod rls;
mod session;

pub use audit::{AuditEntry, AuditLog};
pub use command::{Command, MAX_COMMAND_BYTES};
pub use engine::{
    validate_sql, DistributedEngine, LocalEngine, QueryResult, MAX_SQL_BYTES,
};
pub use error::EngineError;
pub use machine::{apply_command, row_key};
pub use prepared::{bind_parameters, PrepareCache, PreparedStatement};
pub use rls::{apply_rls, materialize_session, Policy, RlsCatalog};
pub use session::SessionContext;
