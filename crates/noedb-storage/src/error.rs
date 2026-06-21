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
    /// An I/O failure while talking to the WAL or SSTable files.
    Io {
        /// Underlying OS error message.
        message: String,
    },
    /// The WAL bytes on disk could not be parsed.
    CorruptWal {
        /// Human-readable reason.
        message: &'static str,
    },
    /// A WAL record failed its CRC-32 check.
    ChecksumMismatch,
    /// SSTable bytes on disk could not be parsed.
    CorruptSstable {
        /// Human-readable reason.
        message: &'static str,
    },
    /// Memory or LSM backlog limits reached — back off and retry.
    ResourceExhausted {
        /// Human-readable reason.
        message: &'static str,
    },
}

impl StorageError {
    /// Construct a [`StorageError::InvalidInput`] error.
    #[must_use]
    pub const fn invalid_input(message: &'static str) -> Self {
        Self::InvalidInput { message }
    }

    /// Construct a [`StorageError::CorruptWal`] error.
    #[must_use]
    pub const fn corrupt_wal(message: &'static str) -> Self {
        Self::CorruptWal { message }
    }

    /// Construct a [`StorageError::ChecksumMismatch`] error.
    #[must_use]
    pub const fn checksum_mismatch() -> Self {
        Self::ChecksumMismatch
    }

    /// Construct a [`StorageError::CorruptSstable`] error.
    #[must_use]
    pub const fn corrupt_sstable(message: &'static str) -> Self {
        Self::CorruptSstable { message }
    }

    /// Construct a [`StorageError::ResourceExhausted`] error.
    #[must_use]
    pub const fn resource_exhausted(message: &'static str) -> Self {
        Self::ResourceExhausted { message }
    }
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        Self::Io {
            message: err.to_string(),
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput { message } => write!(f, "invalid storage input: {message}"),
            Self::Io { message } => write!(f, "storage I/O error: {message}"),
            Self::CorruptWal { message } => write!(f, "corrupt WAL: {message}"),
            Self::ChecksumMismatch => write!(f, "WAL checksum mismatch"),
            Self::CorruptSstable { message } => write!(f, "corrupt SSTable: {message}"),
            Self::ResourceExhausted { message } => write!(f, "resource exhausted: {message}"),
        }
    }
}

impl std::error::Error for StorageError {}
