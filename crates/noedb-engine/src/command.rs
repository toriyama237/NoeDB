//! Replicated state-machine commands (bincode, bounded size).

use serde::{Deserialize, Serialize};

/// Maximum encoded command size (64 KiB).
pub const MAX_COMMAND_BYTES: usize = 64 * 1024;

/// A single mutation applied on every replica after Raft commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Put one column for a row (`table\0row\0col` key).
    Put {
        /// Table name.
        table: String,
        /// Row id.
        row: String,
        /// Column name.
        column: String,
        /// Cell value.
        value: Vec<u8>,
    },
    /// Build a secondary index on one column.
    CreateIndex {
        /// Table name.
        table: String,
        /// Column name.
        column: String,
    },
}

impl Command {
    /// Encode for the Raft log.
    ///
    /// # Errors
    ///
    /// Payload too large or serialization failure.
    pub fn encode(&self) -> Result<Vec<u8>, crate::EngineError> {
        let bytes =
            bincode::serialize(self).map_err(|e| crate::EngineError::Codec(e.to_string()))?;
        if bytes.len() > MAX_COMMAND_BYTES {
            return Err(crate::EngineError::CommandTooLarge);
        }
        Ok(bytes)
    }

    /// Decode from committed log bytes.
    ///
    /// # Errors
    ///
    /// Invalid or oversized payload.
    pub fn decode(bytes: &[u8]) -> Result<Self, crate::EngineError> {
        if bytes.len() > MAX_COMMAND_BYTES {
            return Err(crate::EngineError::CommandTooLarge);
        }
        bincode::deserialize(bytes).map_err(|e| crate::EngineError::Codec(e.to_string()))
    }
}
