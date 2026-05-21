//! TLS 1.3 for NoeDB wire connections (Phase 1, Week 1).
//!
//! - Dev CA + server certs via [`DevCertPem`]
//! - [`server_acceptor_dev`] / [`client_connector_dev`] for tokio-rustls

#![forbid(unsafe_code)]

mod config;
mod dev_certs;
mod error;

pub use config::{
    client_connector_dev, client_connector_untrusted, install_crypto_provider,
    negotiated_version, negotiated_version_client, server_acceptor_dev,
};
pub use dev_certs::DevCertPem;
pub use error::TlsError;

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
