//! Hot-reloadable TLS acceptor (Week 2).

use std::sync::{Arc, RwLock};

use tokio_rustls::TlsAcceptor;

use crate::config::build_server_config_mtls;
use crate::dev_certs::DevCertPem;
use crate::error::TlsError;

/// Server acceptor whose rustls config can be swapped without restarting TCP.
#[derive(Clone)]
pub struct ReloadingAcceptor {
    inner: Arc<RwLock<TlsAcceptor>>,
}

impl ReloadingAcceptor {
    /// Build mTLS acceptor from dev PKI.
    ///
    /// # Errors
    ///
    /// Rustls configuration errors.
    pub fn from_dev_mtls(certs: &DevCertPem) -> Result<Self, TlsError> {
        let config = build_server_config_mtls(certs)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(TlsAcceptor::from(Arc::new(config)))),
        })
    }

    /// Replace TLS settings (e.g. after cert rotation).
    ///
    /// # Errors
    ///
    /// Rustls configuration errors.
    pub fn reload(&self, certs: &DevCertPem) -> Result<(), TlsError> {
        let config = Arc::new(build_server_config_mtls(certs)?);
        {
            let mut guard = self
                .inner
                .write()
                .map_err(|e| TlsError::Handshake(e.to_string()))?;
            *guard = TlsAcceptor::from(config);
        }
        Ok(())
    }

    /// Accept a TCP connection with the current config.
    ///
    /// # Errors
    ///
    /// TLS handshake failures.
    pub async fn accept<IO>(
        &self,
        io: IO,
    ) -> Result<tokio_rustls::server::TlsStream<IO>, std::io::Error>
    where
        IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let acceptor = {
            let guard = self
                .inner
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.clone()
        };
        acceptor.accept(io).await
    }
}
