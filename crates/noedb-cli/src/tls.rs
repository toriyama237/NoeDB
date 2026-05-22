//! TLS / mTLS helpers for the CLI server (Phase 1).

use std::path::{Path, PathBuf};

use noedb_tls::{
    client_connector_mtls_dev, client_connector_mtls_pinned, DevCertPem, ReloadingAcceptor,
};
use tokio_rustls::{TlsAcceptor, TlsConnector};

/// Load or generate dev PKI under `data_dir/tls/` (CA + server + mTLS client).
///
/// # Errors
///
/// I/O or certificate generation failures.
pub(crate) fn load_or_create_dev_certs(
    data_dir: &Path,
    node_id: u64,
) -> Result<DevCertPem, String> {
    let tls_dir = data_dir.join("tls");
    std::fs::create_dir_all(&tls_dir).map_err(|e| e.to_string())?;
    let ca_path = tls_dir.join("ca.pem");
    let cert_path = tls_dir.join("cert.pem");
    let key_path = tls_dir.join("key.pem");
    let client_cert_path = tls_dir.join("client.pem");
    let client_key_path = tls_dir.join("client-key.pem");

    if ca_path.exists()
        && cert_path.exists()
        && key_path.exists()
        && client_cert_path.exists()
        && client_key_path.exists()
    {
        return Ok(DevCertPem {
            ca_pem: std::fs::read_to_string(&ca_path).map_err(|e| e.to_string())?,
            cert_pem: std::fs::read_to_string(&cert_path).map_err(|e| e.to_string())?,
            key_pem: std::fs::read_to_string(&key_path).map_err(|e| e.to_string())?,
            client_cert_pem: std::fs::read_to_string(&client_cert_path)
                .map_err(|e| e.to_string())?,
            client_key_pem: std::fs::read_to_string(&client_key_path).map_err(|e| e.to_string())?,
            client_spiffe_cn: noedb_tls::SpiffeId::cn(node_id),
        });
    }

    let certs = DevCertPem::generate_cluster(node_id).map_err(|e| e.to_string())?;
    std::fs::write(&ca_path, &certs.ca_pem).map_err(|e| e.to_string())?;
    std::fs::write(&cert_path, &certs.cert_pem).map_err(|e| e.to_string())?;
    std::fs::write(&key_path, &certs.key_pem).map_err(|e| e.to_string())?;
    std::fs::write(&client_cert_path, &certs.client_cert_pem).map_err(|e| e.to_string())?;
    std::fs::write(&client_key_path, &certs.client_key_pem).map_err(|e| e.to_string())?;
    Ok(certs)
}

/// Hot-reloadable mTLS acceptor.
pub(crate) fn reloading_acceptor(certs: &DevCertPem) -> Result<ReloadingAcceptor, String> {
    ReloadingAcceptor::from_dev_mtls(certs).map_err(|e| e.to_string())
}

/// One-way TLS acceptor (Week 1 compat).
pub(crate) fn server_acceptor(certs: &DevCertPem) -> Result<TlsAcceptor, String> {
    noedb_tls::server_acceptor_dev(certs).map_err(|e| e.to_string())
}

/// mTLS client connector (trusts dev CA).
pub(crate) fn client_connector(certs: &DevCertPem) -> Result<TlsConnector, String> {
    client_connector_mtls_dev(certs).map_err(|e| e.to_string())
}

/// mTLS client with pinned server leaf (Raft / high-security paths).
pub(crate) fn client_connector_pinned(certs: &DevCertPem) -> Result<TlsConnector, String> {
    client_connector_mtls_pinned(certs).map_err(|e| e.to_string())
}

/// Path to stored CA for clients.
#[must_use]
pub(crate) fn ca_path(data_dir: &Path) -> PathBuf {
    data_dir.join("tls").join("ca.pem")
}
