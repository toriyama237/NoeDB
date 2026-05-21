//! Transaction errors.

use thiserror::Error;

/// Transaction manager failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TxnError {
    /// No active transaction for this session.
    #[error("no active transaction")]
    NoActiveTxn,
    /// Transaction already open on this session.
    #[error("transaction already in progress")]
    TxnAlreadyActive,
    /// Serializable conflict — retry.
    #[error("serialization failure: {0}")]
    SerializationFailure(&'static str),
    /// Deadlock victim — retry.
    #[error("deadlock detected, transaction aborted")]
    DeadlockVictim,
    /// Wait exceeded policy timeout.
    #[error("transaction wait timeout")]
    WaitTimeout,
    /// Storage layer error message.
    #[error("storage: {0}")]
    Storage(String),
}
