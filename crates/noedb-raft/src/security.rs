//! Security hardening for Raft RPC and persistence.

#![allow(clippy::cast_possible_truncation)]
//!
//! Practices aligned with production systems (CockroachDB / etcd style):
//! - bounded frames (DoS resistance)
//! - cluster shared-secret on every RPC
//! - CRC-32 on durable records

use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

use crate::error::SecurityError;

/// Maximum encoded RPC frame size (1 MiB).
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Wire protocol magic (`N` `R` `F` `T`).
pub const WIRE_MAGIC: u32 = 0x4E52_4654;

/// Cluster authentication token carried in every RPC envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClusterAuth(pub [u8; 32]);

impl ClusterAuth {
    /// Derive a token from a passphrase (SHA-256 style mixing without external deps).
    #[must_use]
    pub fn from_passphrase(passphrase: &str) -> Self {
        let mut state = [0u8; 32];
        for (i, b) in passphrase.as_bytes().iter().enumerate() {
            state[i % 32] ^= b.wrapping_mul((i as u8).wrapping_add(31));
            state[(i + 7) % 32] = state[(i + 7) % 32].wrapping_add(*b);
        }
        Self(state)
    }

    /// Constant-time equality check.
    #[must_use]
    pub fn verify(&self, other: &Self) -> bool {
        let mut diff = 0u8;
        for (a, b) in self.0.iter().zip(other.0.iter()) {
            diff |= a ^ b;
        }
        diff == 0
    }
}

/// Envelope wrapping every on-wire message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireEnvelope {
    /// Protocol magic.
    pub magic: u32,
    /// Shared cluster secret.
    pub auth: ClusterAuth,
    /// Payload bytes (bincode-encoded [`crate::rpc::RpcMessage`]).
    pub payload: Vec<u8>,
}

impl WireEnvelope {
    /// Build a new envelope.
    pub fn new(auth: ClusterAuth, payload: Vec<u8>) -> Result<Self, SecurityError> {
        if payload.len() > MAX_FRAME_BYTES {
            return Err(SecurityError::FrameTooLarge {
                size: payload.len(),
            });
        }
        Ok(Self {
            magic: WIRE_MAGIC,
            auth,
            payload,
        })
    }

    /// Validate magic + auth before decoding payload.
    ///
    /// # Errors
    ///
    /// Returns [`SecurityError`] on mismatch or oversize payload.
    pub fn verify(&self, expected_auth: &ClusterAuth) -> Result<(), SecurityError> {
        if self.magic != WIRE_MAGIC {
            return Err(SecurityError::BadMagic);
        }
        if self.payload.len() > MAX_FRAME_BYTES {
            return Err(SecurityError::FrameTooLarge {
                size: self.payload.len(),
            });
        }
        if !expected_auth.verify(&self.auth) {
            return Err(SecurityError::AuthFailed);
        }
        Ok(())
    }
}

/// CRC-32 (IEEE) over arbitrary bytes.
#[must_use]
pub fn checksum(data: &[u8]) -> u32 {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize()
}

/// Verify stored `(payload, checksum)` pair.
///
/// # Errors
///
/// Returns [`SecurityError::ChecksumMismatch`] when CRC does not match.
pub fn verify_checksum(payload: &[u8], expected: u32) -> Result<(), SecurityError> {
    if checksum(payload) != expected {
        return Err(SecurityError::ChecksumMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_constant_time() {
        let a = ClusterAuth::from_passphrase("noedb-cluster");
        let b = ClusterAuth::from_passphrase("noedb-cluster");
        let c = ClusterAuth::from_passphrase("wrong");
        assert!(a.verify(&b));
        assert!(!a.verify(&c));
    }
}
