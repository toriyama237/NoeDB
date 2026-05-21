//! Tonic TLS/mTLS from [`DevCertPem`] (TLS 1.3, mutual auth).

use noedb_tls::{install_crypto_provider, DevCertPem};
use tonic::transport::{Certificate, ClientTlsConfig, Identity, ServerTlsConfig};

/// Default gRPC listen address (HTTP/2, separate from legacy TCP `5433`).
pub const DEFAULT_GRPC_ADDR: &str = "127.0.0.1:5434";

/// Max encoded gRPC message (matches Raft frame cap).
pub const MAX_GRPC_BYTES: usize = noedb_raft::MAX_FRAME_BYTES;

/// mTLS server TLS config (requires client cert signed by dev CA).
///
/// # Errors
///
/// PEM parse failures.
pub fn server_tls_mtls(certs: &DevCertPem) -> Result<ServerTlsConfig, String> {
    install_crypto_provider();
    let identity = Identity::from_pem(certs.cert_pem.as_bytes(), certs.key_pem.as_bytes());
    let client_ca = Certificate::from_pem(certs.ca_pem.as_bytes());
    Ok(ServerTlsConfig::new()
        .identity(identity)
        .client_ca_root(client_ca))
}

/// One-way TLS server (Week 1 compat).
pub fn server_tls_one_way(certs: &DevCertPem) -> Result<ServerTlsConfig, String> {
    install_crypto_provider();
    let identity = Identity::from_pem(certs.cert_pem.as_bytes(), certs.key_pem.as_bytes());
    Ok(ServerTlsConfig::new().identity(identity))
}

/// Peer host for TLS verification (must match server cert SAN).
#[must_use]
pub fn peer_host_from_addr(addr: &str) -> &str {
    addr.split(':').next().unwrap_or("localhost")
}

/// mTLS client (presents SPIFFE client cert, trusts dev CA).
pub fn client_tls_mtls(certs: &DevCertPem, peer_host: &str) -> Result<ClientTlsConfig, String> {
    install_crypto_provider();
    let domain = std::env::var("NOEDB_GRPC_DOMAIN").unwrap_or_else(|_| peer_host.to_string());
    let ca = Certificate::from_pem(certs.ca_pem.as_bytes());
    let identity = Identity::from_pem(certs.client_cert_pem.as_bytes(), certs.client_key_pem.as_bytes());
    Ok(ClientTlsConfig::new()
        .domain_name(domain)
        .ca_certificate(ca)
        .identity(identity))
}

/// TLS client without client cert (one-way).
pub fn client_tls_one_way(certs: &DevCertPem, peer_host: &str) -> Result<ClientTlsConfig, String> {
    install_crypto_provider();
    let domain = std::env::var("NOEDB_GRPC_DOMAIN").unwrap_or_else(|_| peer_host.to_string());
    let ca = Certificate::from_pem(certs.ca_pem.as_bytes());
    Ok(ClientTlsConfig::new().domain_name(domain).ca_certificate(ca))
}
