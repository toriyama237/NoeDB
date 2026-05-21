//! TLS errors.

use thiserror::Error;

/// TLS configuration or handshake failures.
#[derive(Debug, Error)]
pub enum TlsError {
    /// Certificate generation failed.
    #[error("certificate generation: {0}")]
    CertGen(String),
    /// PEM parsing failed.
    #[error("pem: {0}")]
    Pem(String),
    /// Rustls configuration error.
    #[error("rustls: {0}")]
    Rustls(#[from] rustls::Error),
    /// I/O while reading cert files.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Handshake or verification failure.
    #[error("handshake: {0}")]
    Handshake(String),
}
