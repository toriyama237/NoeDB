//! SPIFFE CN enforcement layered on Web PKI client verification.

use std::sync::Arc;

use rustls::pki_types::{CertificateDer, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::WebPkiClientVerifier;
use rustls::{DigitallySignedStruct, DistinguishedName, Error, SignatureScheme};
use x509_parser::prelude::FromDer;
use x509_parser::prelude::X509Certificate;

use crate::error::TlsError;
use crate::identity::SpiffeId;

/// Web PKI + mandatory SPIFFE-style CN on presented client certificates.
#[derive(Debug)]
pub struct SpiffeClientVerifier {
    inner: Arc<WebPkiClientVerifier>,
}

impl SpiffeClientVerifier {
    /// Wrap the dev-CA Web PKI verifier with SPIFFE CN checks.
    ///
    /// # Errors
    ///
    /// Rustls verifier construction failures.
    pub fn new(inner: Arc<WebPkiClientVerifier>) -> Self {
        Self { inner }
    }

    fn verify_spiffe_cn(end_entity: &CertificateDer<'_>) -> Result<(), Error> {
        let (_, cert) = X509Certificate::from_der(end_entity.as_ref())
            .map_err(|e| Error::General(format!("bad client cert: {e}")))?;
        let cn = cert
            .subject()
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .ok_or_else(|| Error::General("client cert missing CN".into()))?;
        SpiffeId::parse_cn(cn).map_err(|e| Error::General(e.to_string()))?;
        Ok(())
    }
}

impl ClientCertVerifier for SpiffeClientVerifier {
    fn offer_client_auth(&self) -> bool {
        self.inner.offer_client_auth()
    }

    fn client_auth_mandatory(&self) -> bool {
        self.inner.client_auth_mandatory()
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        self.inner
            .verify_client_cert(end_entity, intermediates, now)?;
        Self::verify_spiffe_cn(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<ClientCertVerified, Error> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<ClientCertVerified, Error> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dev_certs::DevCertPem;
    use rustls::RootCertStore;

    #[test]
    fn spiffe_verifier_accepts_cluster_client_cert() {
        let certs = DevCertPem::generate_cluster(2).unwrap();
        let ca = certs.ca_cert().unwrap();
        let mut roots = RootCertStore::empty();
        roots.add(ca).unwrap();
        let webpki = WebPkiClientVerifier::builder(roots.into())
            .build()
            .unwrap();
        let verifier = SpiffeClientVerifier::new(webpki);
        let client = certs.client_identity().unwrap().0[0].clone();
        let now = UnixTime::now();
        assert!(verifier
            .verify_client_cert(&client, &[], now)
            .is_ok());
    }
}
