//! Engine errors.

use noedb_planner::ExecError;
use noedb_raft::RaftError;
use noedb_storage::StorageError;
use thiserror::Error;

/// Errors from the integrated SQL engine.
#[derive(Debug, Error)]
pub enum EngineError {
    /// SQL string failed validation (length, charset).
    #[error("invalid SQL input: {0}")]
    InvalidSql(&'static str),
    /// Parse failure.
    #[error("parse error: {0}")]
    Parse(#[from] noedb_parser::ParseError),
    /// Execution failure.
    #[error("execution error: {0:?}")]
    Exec(ExecError),
    /// Raft cluster error.
    #[error("raft error: {0}")]
    Raft(#[from] RaftError),
    /// Storage error.
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    /// Serialization error.
    #[error("codec error: {0}")]
    Codec(String),
    /// Replicated command too large.
    #[error("command exceeds size limit")]
    CommandTooLarge,
    /// No Raft leader available.
    #[error("no raft leader")]
    NoLeader,
    /// Statement not supported on the distributed path.
    #[error("unsupported statement for distributed engine")]
    UnsupportedStatement,
    /// MVCC / SSI conflict — safe to retry the transaction.
    #[error("serialization failure: {0}")]
    SerializationFailure(String),
}

impl From<ExecError> for EngineError {
    fn from(e: ExecError) -> Self {
        Self::Exec(e)
    }
}
