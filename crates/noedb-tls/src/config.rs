//! Rustls server and client configuration (TLS 1.3 + mTLS).

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::WebPkiClientVerifier;
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, ServerConfig,
    ServerConnection, SignatureScheme,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use crate::dev_certs::DevCertPem;
use crate::error::TlsError;

/// Install the ring crypto provider (required once per process).
pub fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn ca_root_store(certs: &DevCertPem) -> Result<RootCertStore, TlsError> {
    let ca = certs.ca_cert()?;
    let mut roots = RootCertStore::empty();
    roots.add(ca)?;
    Ok(roots)
}

/// Build mTLS [`ServerConfig`] (requires client certs signed by dev CA).
///
/// # Errors
///
/// Rustls configuration errors.
pub fn build_server_config_mtls(certs: &DevCertPem) -> Result<ServerConfig, TlsError> {
    install_crypto_provider();
    let roots = ca_root_store(certs)?;
    let client_verifier = WebPkiClientVerifier::builder(roots.into())
        .build()
        .map_err(|e| TlsError::Handshake(e.to_string()))?;
    let (chain, key) = certs.server_identity()?;
    ServerConfig::builder()
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(chain, key)
        .map_err(Into::into)
}

/// Build one-way TLS server (Week 1 compat tests).
///
/// # Errors
///
/// Rustls configuration errors.
pub fn build_server_config_tls(certs: &DevCertPem) -> Result<ServerConfig, TlsError> {
    install_crypto_provider();
    let (chain, key) = certs.server_identity()?;
    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(Into::into)
}

/// Build mTLS [`ClientConfig`] (presents client identity, trusts dev CA).
///
/// # Errors
///
/// Rustls configuration errors.
pub fn build_client_config_mtls(certs: &DevCertPem) -> Result<ClientConfig, TlsError> {
    install_crypto_provider();
    let roots = ca_root_store(certs)?;
    let (client_chain, client_key) = certs.client_identity()?;
    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(client_chain, client_key)?)
}

/// Build TLS client trusting dev CA only (no client cert).
///
/// # Errors
///
/// Rustls configuration errors.
pub fn build_client_config_tls(certs: &DevCertPem) -> Result<ClientConfig, TlsError> {
    install_crypto_provider();
    let roots = ca_root_store(certs)?;
    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

/// Build a TLS 1.3 server acceptor (one-way TLS).
pub fn server_acceptor_dev(certs: &DevCertPem) -> Result<TlsAcceptor, TlsError> {
    Ok(TlsAcceptor::from(Arc::new(build_server_config_tls(certs)?)))
}

/// Build an mTLS server acceptor (client cert required).
pub fn server_acceptor_mtls_dev(certs: &DevCertPem) -> Result<TlsAcceptor, TlsError> {
    Ok(TlsAcceptor::from(Arc::new(build_server_config_mtls(
        certs,
    )?)))
}

/// Build a TLS client connector (one-way).
pub fn client_connector_dev(certs: &DevCertPem) -> Result<TlsConnector, TlsError> {
    Ok(TlsConnector::from(Arc::new(build_client_config_tls(
        certs,
    )?)))
}

/// Build an mTLS client connector.
pub fn client_connector_mtls_dev(certs: &DevCertPem) -> Result<TlsConnector, TlsError> {
    Ok(TlsConnector::from(Arc::new(build_client_config_mtls(
        certs,
    )?)))
}

/// Connector that rejects unknown CAs (for negative tests).
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
