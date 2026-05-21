//! Raft error types.

use thiserror::Error;

/// Consensus-layer failures.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RaftError {
    /// Persistent storage failure.
    #[error("storage: {0}")]
    Storage(#[from] StorageError),
    /// Network / transport failure.
    #[error("transport: {0}")]
    Transport(String),
    /// RPC failed validation (size, auth, CRC).
    #[error("security: {0}")]
    Security(#[from] SecurityError),
    /// Internal invariant violated.
    #[error("internal: {0}")]
    Internal(String),
}

impl RaftError {
    /// Build an internal error.
    #[must_use]
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// Storage backend errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StorageError {
    /// I/O error with context.
    #[error("io: {message}")]
    Io {
        /// Detail.
        message: String,
    },
    /// Corrupt or tampered on-disk record.
    #[error("corrupt: {0}")]
    Corrupt(String),
    /// Snapshot install failed.
    #[error("snapshot: {0}")]
    Snapshot(String),
}

/// Security validation errors.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SecurityError {
    /// Frame exceeds [`crate::security::MAX_FRAME_BYTES`].
    #[error("frame too large: {size} bytes")]
    FrameTooLarge {
        /// Received size.
        size: usize,
    },
    /// Cluster auth token mismatch.
    #[error("cluster authentication failed")]
    AuthFailed,
    /// CRC mismatch on a persisted record.
    #[error("checksum mismatch")]
    ChecksumMismatch,
    /// Unknown protocol magic.
    #[error("bad wire magic")]
    BadMagic,
}

impl StorageError {
    /// Wrap a standard I/O error.
    #[must_use]
    pub fn io(err: impl std::fmt::Display) -> Self {
        Self::Io {
            message: err.to_string(),
        }
    }
}
