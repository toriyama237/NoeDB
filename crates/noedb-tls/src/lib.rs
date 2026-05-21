//! TLS 1.3 and mTLS for NoeDB (Phase 1).
//!
//! - Dev PKI via [`DevCertPem`] (CA + server + SPIFFE client identity)
//! - mTLS: [`server_acceptor_mtls_dev`] / [`client_connector_mtls_dev`]
//! - Pinning: [`pinned_mtls_connector`]
//! - Hot reload: [`ReloadingAcceptor`]

#![forbid(unsafe_code)]

mod config;
mod dev_certs;
mod error;
mod identity;
mod pin;
mod reload;

pub use config::{
    build_client_config_mtls, build_client_config_tls, build_server_config_mtls,
    build_server_config_tls, client_connector_dev, client_connector_mtls_dev,
    client_connector_untrusted, install_crypto_provider, negotiated_version,
    negotiated_version_client, server_acceptor_dev, server_acceptor_mtls_dev,
};
pub use dev_certs::DevCertPem;
pub use error::TlsError;
pub use identity::{SpiffeId, SPIFFE_PREFIX};
pub use pin::client_connector_mtls_pinned;
pub use reload::ReloadingAcceptor;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    use super::*;

    #[tokio::test]
    async fn tls13_handshake_succeeds() {
        let certs = DevCertPem::generate_localhost().unwrap();
        let acceptor = server_acceptor_dev(&certs).unwrap();
        let connector = client_connector_dev(&certs).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let tls = acceptor.accept(tcp).await.unwrap();
            negotiated_version(tls.get_ref().1)
                .expect("tls version")
                .to_string()
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let server_name = "localhost".try_into().unwrap();
        let mut tls = connector.connect(server_name, tcp).await.unwrap();
        let version = negotiated_version_client(tls.get_ref().1).unwrap();
        assert_eq!(version, "TLSv1.3");

        tls.write_all(b"ping").await.unwrap();
        tls.flush().await.unwrap();

        let server_version = server.await.unwrap();
        assert_eq!(server_version, "TLSv1.3");
    }

    #[tokio::test]
    async fn wrong_ca_rejects_handshake() {
        let good = DevCertPem::generate_localhost().unwrap();
        let bad = DevCertPem::generate_localhost().unwrap();
        let acceptor = server_acceptor_dev(&good).unwrap();
        let connector = client_connector_dev(&bad).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                let _ = acceptor.accept(tcp).await;
            }
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let server_name = "localhost".try_into().unwrap();
        let result = connector.connect(server_name, tcp).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mtls_handshake_requires_client_cert() {
        let certs = DevCertPem::generate_cluster(2).unwrap();
        let acceptor = server_acceptor_mtls_dev(&certs).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let _ = acceptor.accept(tcp).await;
        });

        // Client cert from another CA must fail against this mTLS server.
        let other = DevCertPem::generate_cluster(99).unwrap();
        let bad = client_connector_mtls_dev(&other).unwrap();
        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let name = "localhost".try_into().unwrap();
        assert!(bad.connect(name, tcp).await.is_err());
    }

    #[tokio::test]
    async fn mtls_handshake_succeeds() {
        let certs = DevCertPem::generate_cluster(3).unwrap();
        let acceptor = server_acceptor_mtls_dev(&certs).unwrap();
        let connector = client_connector_mtls_dev(&certs).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            acceptor.accept(tcp).await.unwrap();
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let name = "localhost".try_into().unwrap();
        connector.connect(name, tcp).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn acceptor_reload_after_rotation() {
        let old = DevCertPem::generate_cluster(1).unwrap();
        let new = DevCertPem::generate_cluster(1).unwrap();
        let acceptor = ReloadingAcceptor::from_dev_mtls(&old).unwrap();
        acceptor.reload(&new).unwrap();

        let connector = client_connector_mtls_dev(&new).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            acceptor.accept(tcp).await.unwrap();
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let name = "localhost".try_into().unwrap();
        connector.connect(name, tcp).await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn pinned_connector_rejects_wrong_server() {
        let a = DevCertPem::generate_cluster(1).unwrap();
        let b = DevCertPem::generate_cluster(2).unwrap();
        let acceptor = server_acceptor_mtls_dev(&a).unwrap();
        let pinned = client_connector_mtls_pinned(&b).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                let _ = acceptor.accept(tcp).await;
            }
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let name = "localhost".try_into().unwrap();
        assert!(pinned.connect(name, tcp).await.is_err());
    }

    #[test]
    fn spiffe_cn_roundtrip() {
        let cn = SpiffeId::cn(7);
        let id = SpiffeId::parse_cn(&cn).unwrap();
        assert_eq!(id.node_id, 7);
    }

    #[tokio::test]
    async fn untrusted_verifier_rejects() {
        let certs = DevCertPem::generate_localhost().unwrap();
        let acceptor = server_acceptor_dev(&certs).unwrap();
        let connector = client_connector_untrusted().unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await {
                let _ = acceptor.accept(tcp).await;
            }
        });

        let tcp = tokio::net::TcpStream::connect(addr).await.unwrap();
        let server_name = "localhost".try_into().unwrap();
        let result = connector.connect(server_name, tcp).await;
        assert!(result.is_err());
    }
}
