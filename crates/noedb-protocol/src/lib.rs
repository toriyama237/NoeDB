//! Secure framed wire protocol for NoeDB clients (Phase 5, Week 47).
//!
//! - Magic + version header
//! - Length-prefixed frames (max 1 MiB, shared with Raft)
//! - [`ClusterAuth`] on every request
//! - Bincode payloads

#![forbid(unsafe_code)]

use noedb_raft::{ClusterAuth, SecurityError, WireEnvelope, MAX_FRAME_BYTES};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Protocol version byte on the wire.
pub const PROTOCOL_VERSION: u8 = 1;

/// Client SQL request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Request {
    /// Run SQL (bounded by engine).
    Sql {
        /// Query text.
        query: String,
    },
    /// Return `EXPLAIN` for a `SELECT`.
    Explain {
        /// Query text.
        query: String,
    },
    /// Health check.
    Ping,
}

/// Server response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Response {
    /// Tabular result.
    Ok {
        /// Column names.
        columns: Vec<String>,
        /// Rows.
        rows: Vec<Vec<String>>,
    },
    /// `EXPLAIN` text.
    Explain {
        /// Plan string.
        text: String,
    },
    /// Pong.
    Pong,
    /// Application error.
    Error {
        /// Short code.
        code: u16,
        /// Human-readable message.
        message: String,
    },
}

/// Wire encode/decode errors.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// Frame or auth failure from security layer.
    #[error("security: {0}")]
    Security(#[from] SecurityError),
    /// Serialization failure.
    #[error("codec: {0}")]
    Codec(String),
    /// Unsupported protocol version.
    #[error("unsupported protocol version {0}")]
    BadVersion(u8),
    /// Frame magic mismatch.
    #[error("bad frame magic")]
    BadMagic,
}

/// Encode a request frame (auth + bincode).
///
/// # Errors
///
/// Payload too large or serialization failure.
pub fn encode_request(auth: &ClusterAuth, req: &Request) -> Result<Vec<u8>, ProtocolError> {
    let payload = bincode::serialize(req).map_err(|e| ProtocolError::Codec(e.to_string()))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Security(SecurityError::FrameTooLarge {
            size: payload.len(),
        }));
    }
    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(PROTOCOL_VERSION);
    frame.extend_from_slice(&payload);
    let envelope = WireEnvelope::new(auth.clone(), frame)?;
    bincode::serialize(&envelope).map_err(|e| ProtocolError::Codec(e.to_string()))
}

/// Decode a request frame.
///
/// # Errors
///
/// Auth, size, or codec failures.
pub fn decode_request(auth: &ClusterAuth, bytes: &[u8]) -> Result<Request, ProtocolError> {
    let env: WireEnvelope =
        bincode::deserialize(bytes).map_err(|e| ProtocolError::Codec(e.to_string()))?;
    env.verify(auth)?;
    if env.payload.is_empty() {
        return Err(ProtocolError::Codec("empty payload".into()));
    }
    let version = env.payload[0];
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::BadVersion(version));
    }
    let req: Request =
        bincode::deserialize(&env.payload[1..]).map_err(|e| ProtocolError::Codec(e.to_string()))?;
    Ok(req)
}

/// Encode a response frame.
///
/// # Errors
///
/// Payload too large or serialization failure.
pub fn encode_response(auth: &ClusterAuth, resp: &Response) -> Result<Vec<u8>, ProtocolError> {
    let payload = bincode::serialize(resp).map_err(|e| ProtocolError::Codec(e.to_string()))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::Security(SecurityError::FrameTooLarge {
            size: payload.len(),
        }));
    }
    let mut frame = Vec::with_capacity(1 + payload.len());
    frame.push(PROTOCOL_VERSION);
    frame.extend_from_slice(&payload);
    let envelope = WireEnvelope::new(auth.clone(), frame)?;
    bincode::serialize(&envelope).map_err(|e| ProtocolError::Codec(e.to_string()))
}

/// Decode a response frame.
///
/// # Errors
///
/// Auth, size, or codec failures.
pub fn decode_response(auth: &ClusterAuth, bytes: &[u8]) -> Result<Response, ProtocolError> {
    let env: WireEnvelope =
        bincode::deserialize(bytes).map_err(|e| ProtocolError::Codec(e.to_string()))?;
    env.verify(auth)?;
    if env.payload.is_empty() {
        return Err(ProtocolError::Codec("empty payload".into()));
    }
    let version = env.payload[0];
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::BadVersion(version));
    }
    let resp: Response =
        bincode::deserialize(&env.payload[1..]).map_err(|e| ProtocolError::Codec(e.to_string()))?;
    Ok(resp)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_sql_request() {
        let auth = ClusterAuth::from_passphrase("noedb-dev");
        let req = Request::Sql {
            query: "SELECT 1".to_string(),
        };
        let bytes = encode_request(&auth, &req).unwrap();
        let back = decode_request(&auth, &bytes).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn rejects_wrong_auth() {
        let a = ClusterAuth::from_passphrase("a");
        let b = ClusterAuth::from_passphrase("b");
        let bytes = encode_request(&a, &Request::Ping).unwrap();
        assert!(decode_request(&b, &bytes).is_err());
    }
}
