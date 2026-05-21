//! Storage engine errors.

use core::fmt;

/// An error produced by the storage layer.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum StorageError {
    /// A key or value violated engine invariants (empty key, size limit, …).
    InvalidInput {
        /// Human-readable reason.
        message: &'static str,
    },
}

impl StorageError {
    /// Construct an [`InvalidInput`] error.
    #[must_use]
    pub const fn invalid_input(message: &'static str) -> Self {
        Self::InvalidInput { message }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { message } => write!(f, "invalid storage input: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}
