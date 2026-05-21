//! TLS helpers for the CLI server (Phase 1, Week 1).

use std::path::{Path, PathBuf};

use noedb_tls::{client_connector_dev, server_acceptor_dev, DevCertPem};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Load or generate dev TLS material under `data_dir/tls/`.
///
/// # Errors
///
/// I/O or certificate generation failures.
pub(crate) fn load_or_create_dev_certs(data_dir: &Path) -> Result<DevCertPem, String> {
    let tls_dir = data_dir.join("tls");
    std::fs::create_dir_all(&tls_dir).map_err(|e| e.to_string())?;
    let ca_path = tls_dir.join("ca.pem");
    let cert_path = tls_dir.join("cert.pem");
    let key_path = tls_dir.join("key.pem");

    if ca_path.exists() && cert_path.exists() && key_path.exists() {
        return Ok(DevCertPem {
            ca_pem: std::fs::read_to_string(&ca_path).map_err(|e| e.to_string())?,
            cert_pem: std::fs::read_to_string(&cert_path).map_err(|e| e.to_string())?,
            key_pem: std::fs::read_to_string(&key_path).map_err(|e| e.to_string())?,
        });
    }

    let certs = DevCertPem::generate_localhost().map_err(|e| e.to_string())?;
    std::fs::write(&ca_path, &certs.ca_pem).map_err(|e| e.to_string())?;
    std::fs::write(&cert_path, &certs.cert_pem).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, &certs.key_pem).map_err(|e| e.to_string())?;
    Ok(certs)
}

/// Build server TLS acceptor from dev certs.
pub(crate) fn server_acceptor(certs: &DevCertPem) -> Result<TlsAcceptor, String> {
    server_acceptor_dev(certs).map_err(|e| e.to_string())
}

/// Build client connector trusting dev CA.
pub(crate) fn client_connector(certs: &DevCertPem) -> Result<TlsConnector, String> {
    client_connector_dev(certs).map_err(|e| e.to_string())
}

/// Path to stored CA for clients.
#[must_use]
pub(crate) fn ca_path(data_dir: &Path) -> PathBuf {
    data_dir.join("tls").join("ca.pem")
}
