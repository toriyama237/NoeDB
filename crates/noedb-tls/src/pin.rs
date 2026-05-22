//! Certificate pinning for cluster clients (Week 2).

use std::sync::Arc;

use rustls::RootCertStore;
use tokio_rustls::TlsConnector;

use crate::config::install_crypto_provider;
use crate::dev_certs::DevCertPem;
use crate::error::TlsError;

/// mTLS client that trusts **only** the pinned server leaf (not the full CA).
///
/// # Errors
///
/// Rustls configuration errors.
pub fn client_connector_mtls_pinned(certs: &DevCertPem) -> Result<TlsConnector, TlsError> {
    install_crypto_provider();
    let leaf = certs.server_leaf_der()?;
    let mut roots = RootCertStore::empty();
    roots.add(leaf)?;
    let (client_chain, client_key) = certs.client_identity()?;
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(client_chain, client_key)?;
    Ok(TlsConnector::from(Arc::new(config)))
}
