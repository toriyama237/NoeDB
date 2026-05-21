//! Rustls server and client configuration (TLS 1.3).

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, ServerConfig, SignatureScheme};
use rustls::{ClientConnection, ServerConnection};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::dev_certs::DevCertPem;
use crate::error::TlsError;

/// Install the ring crypto provider (required once per process).
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

/// Build a TLS 1.3 server acceptor from dev certificates.
///
/// # Errors
///
/// Rustls configuration errors.
pub fn server_acceptor_dev(certs: &DevCertPem) -> Result<TlsAcceptor, TlsError> {
    install_crypto_provider();
    let (chain, key) = certs.server_identity()?;
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key)?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

/// Build a TLS client connector trusting the dev CA only.
///
/// # Errors
///
/// Rustls configuration errors.
pub fn client_connector_dev(certs: &DevCertPem) -> Result<TlsConnector, TlsError> {
    install_crypto_provider();
    let ca = certs.ca_cert()?;
    let mut roots = RootCertStore::empty();
    roots.add(ca)?;
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

/// Connector that rejects unknown CAs (for negative tests).
///
/// # Errors
///
/// Rustls configuration errors.
pub fn client_connector_untrusted() -> Result<TlsConnector, TlsError> {
    install_crypto_provider();
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(RejectAllVerifier))
        .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

#[derive(Debug)]
struct RejectAllVerifier;

impl ServerCertVerifier for RejectAllVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Err(rustls::Error::General("untrusted".into()))
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("untrusted".into()))
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("untrusted".into()))
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

/// Negotiated protocol version string after handshake.
#[must_use]
pub fn negotiated_version(conn: &ServerConnection) -> Option<&'static str> {
    match conn.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => Some("TLSv1.3"),
        Some(rustls::ProtocolVersion::TLSv1_2) => Some("TLSv1.2"),
        _ => None,
    }
}

/// Client-side negotiated version.
#[must_use]
pub fn negotiated_version_client(conn: &ClientConnection) -> Option<&'static str> {
    match conn.protocol_version() {
        Some(rustls::ProtocolVersion::TLSv1_3) => Some("TLSv1.3"),
        Some(rustls::ProtocolVersion::TLSv1_2) => Some("TLSv1.2"),
        _ => None,
    }
}
