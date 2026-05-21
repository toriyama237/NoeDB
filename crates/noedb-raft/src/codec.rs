//! Binary codec for RPC frames (bincode + security envelope).

use crate::error::{RaftError, SecurityError};
use crate::rpc::RpcMessage;
use crate::security::{ClusterAuth, WireEnvelope, WIRE_MAGIC};

/// Encode an RPC for the wire.
///
/// # Errors
///
/// Security or serialization failures.
pub fn encode_message(auth: &ClusterAuth, msg: &RpcMessage) -> Result<Vec<u8>, RaftError> {
    let payload = bincode::serialize(msg).map_err(|e| RaftError::Transport(e.to_string()))?;
    let env = WireEnvelope::new(auth.clone(), payload).map_err(RaftError::Security)?;
    bincode::serialize(&env).map_err(|e| RaftError::Transport(e.to_string()))
}

/// Decode and verify a frame.
///
/// # Errors
///
/// Security or deserialization failures.
pub fn decode_message(auth: &ClusterAuth, frame: &[u8]) -> Result<RpcMessage, RaftError> {
    let env: WireEnvelope =
        bincode::deserialize(frame).map_err(|e| RaftError::Transport(e.to_string()))?;
    if env.magic != WIRE_MAGIC {
        return Err(RaftError::Security(SecurityError::BadMagic));
    }
    env.verify(auth).map_err(RaftError::Security)?;
    bincode::deserialize(&env.payload).map_err(|e| RaftError::Transport(e.to_string()))
}
