//! At-rest value encryption (XChaCha20-Poly1305 AEAD).
//!
//! Column values can be sealed before they reach the LSM so that an attacker
//! with raw access to SST files or backups sees only ciphertext. Each record
//! gets a fresh random 192-bit nonce, and the cell coordinates
//! (`table/row/column`) are bound as associated data so a ciphertext cannot be
//! silently relocated to a different cell (confused-deputy / swap attack).
//!
//! Wire format of a sealed value:
//! ```text
//! [ 1 byte version ][ 24 byte nonce ][ ciphertext + 16 byte tag ]
//! ```

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::error::EngineError;

/// Sealed-value format version (first byte).
const VERSION: u8 = 1;
/// XChaCha20 nonce length in bytes.
const NONCE_LEN: usize = 24;
/// Minimum sealed length: version + nonce + AEAD tag (16).
const MIN_SEALED: usize = 1 + NONCE_LEN + 16;

/// A data-encryption key for at-rest sealing.
#[derive(Clone)]
pub struct DataKey {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for DataKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DataKey(****)")
    }
}

impl DataKey {
    /// Derive a key from arbitrary secret material via SHA-256.
    #[must_use]
    pub fn from_secret(secret: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(b"noedb-at-rest-v1");
        h.update(secret);
        let digest = h.finalize();
        let key = Key::from_slice(&digest);
        Self {
            cipher: XChaCha20Poly1305::new(key),
        }
    }

    /// Derive a key from the `NOEDB_DATA_KEY` env var, if set.
    #[must_use]
    pub fn from_env() -> Option<Self> {
        std::env::var("NOEDB_DATA_KEY")
            .ok()
            .filter(|s| !s.is_empty())
            .map(|s| Self::from_secret(s.as_bytes()))
    }

    /// Seal `plaintext` for the cell identified by `aad`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Crypto`] on AEAD failure (should not happen for valid keys).
    pub fn seal(&self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, EngineError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = XNonce::from_slice(&nonce_bytes);
        let ct = self
            .cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| EngineError::Crypto("seal failed"))?;
        let mut out = Vec::with_capacity(1 + NONCE_LEN + ct.len());
        out.push(VERSION);
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    /// Open a value previously produced by [`DataKey::seal`] with the same `aad`.
    ///
    /// # Errors
    ///
    /// [`EngineError::Crypto`] on malformed input, version mismatch, wrong key,
    /// tampered ciphertext, or mismatched associated data.
    pub fn open(&self, aad: &[u8], sealed: &[u8]) -> Result<Vec<u8>, EngineError> {
        if sealed.len() < MIN_SEALED {
            return Err(EngineError::Crypto("sealed value too short"));
        }
        if sealed[0] != VERSION {
            return Err(EngineError::Crypto("unknown seal version"));
        }
        let nonce = XNonce::from_slice(&sealed[1..1 + NONCE_LEN]);
        let ct = &sealed[1 + NONCE_LEN..];
        self.cipher
            .decrypt(nonce, Payload { msg: ct, aad })
            .map_err(|_| EngineError::Crypto("open failed (wrong key or tampered)"))
    }
}

/// Associated data binding a ciphertext to its logical cell location.
#[must_use]
pub fn cell_aad(table: &str, row: &str, column: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(table.len() + row.len() + column.len() + 2);
    aad.extend_from_slice(table.as_bytes());
    aad.push(0);
    aad.extend_from_slice(row.as_bytes());
    aad.push(0);
    aad.extend_from_slice(column.as_bytes());
    aad
}

/// Whether `value` looks like a sealed blob (cheap heuristic on header).
#[must_use]
pub fn is_sealed(value: &[u8]) -> bool {
    value.len() >= MIN_SEALED && value[0] == VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_recovers_plaintext() {
        let key = DataKey::from_secret(b"correct horse battery staple");
        let aad = cell_aad("accounts", "42", "balance");
        let sealed = key.seal(&aad, b"1000.00").unwrap();
        assert!(is_sealed(&sealed));
        let opened = key.open(&aad, &sealed).unwrap();
        assert_eq!(opened, b"1000.00");
    }

    #[test]
    fn wrong_key_fails() {
        let k1 = DataKey::from_secret(b"key-one");
        let k2 = DataKey::from_secret(b"key-two");
        let aad = cell_aad("t", "r", "c");
        let sealed = k1.seal(&aad, b"secret").unwrap();
        assert!(k2.open(&aad, &sealed).is_err());
    }

    #[test]
    fn relocated_ciphertext_fails() {
        let key = DataKey::from_secret(b"k");
        let aad1 = cell_aad("t", "row1", "c");
        let aad2 = cell_aad("t", "row2", "c");
        let sealed = key.seal(&aad1, b"v").unwrap();
        assert!(key.open(&aad2, &sealed).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = DataKey::from_secret(b"k");
        let aad = cell_aad("t", "r", "c");
        let mut sealed = key.seal(&aad, b"hello").unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 0xff;
        assert!(key.open(&aad, &sealed).is_err());
    }

    #[test]
    fn distinct_nonces_per_seal() {
        let key = DataKey::from_secret(b"k");
        let aad = cell_aad("t", "r", "c");
        let a = key.seal(&aad, b"same").unwrap();
        let b = key.seal(&aad, b"same").unwrap();
        assert_ne!(a, b, "nonce reuse would make ciphertexts identical");
    }

    #[test]
    fn short_input_rejected() {
        let key = DataKey::from_secret(b"k");
        assert!(key.open(b"", &[1, 2, 3]).is_err());
    }
}
