//! Development self-signed certificates (rcgen).

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::error::TlsError;

/// PEM-encoded dev CA + server cert + private key.
#[derive(Debug, Clone)]
pub struct DevCertPem {
    /// CA certificate PEM (for clients).
    pub ca_pem: String,
    /// Server certificate PEM.
    pub cert_pem: String,
    /// Server private key PEM.
    pub key_pem: String,
}

impl DevCertPem {
    /// Generate a localhost dev CA and server certificate (TLS 1.3).
    ///
    /// # Errors
    ///
    /// rcgen or key generation failures.
    pub fn generate_localhost() -> Result<Self, TlsError> {
        let key_pair = KeyPair::generate().map_err(|e| TlsError::CertGen(e.to_string()))?;

        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "NoeDB Dev CA");
        ca_params.distinguished_name = ca_dn;
        let ca_cert = ca_params
            .self_signed(&key_pair)
            .map_err(|e| TlsError::CertGen(e.to_string()))?;

        let mut server_params = CertificateParams::default();
        server_params.subject_alt_names = vec![
            SanType::DnsName("localhost".try_into().map_err(|e| TlsError::CertGen(format!("{e:?}")))?),
            SanType::IpAddress(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
        ];
        let mut server_dn = DistinguishedName::new();
        server_dn.push(DnType::CommonName, "noedb.local");
        server_params.distinguished_name = server_dn;

        let server_cert = server_params
            .signed_by(&key_pair, &ca_cert, &key_pair)
            .map_err(|e| TlsError::CertGen(e.to_string()))?;

        Ok(Self {
            ca_pem: ca_cert.pem(),
            cert_pem: server_cert.pem(),
            key_pem: key_pair.serialize_pem(),
        })
    }

    /// Parse server cert + key for rustls.
    ///
    /// # Errors
    ///
    /// PEM or rustls parse errors.
    pub fn server_identity(&self) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TlsError> {
        let certs = pem_certs(&self.cert_pem)?;
        let key = pem_key(&self.key_pem)?;
        Ok((certs, key))
    }

    /// Parse CA cert for client trust store.
    ///
    /// # Errors
    ///
    /// PEM parse errors.
    pub fn ca_cert(&self) -> Result<CertificateDer<'static>, TlsError> {
        let mut certs = pem_certs(&self.ca_pem)?;
        certs.pop().ok_or_else(|| TlsError::Pem("empty CA".into()))
    }
}

fn pem_certs(pem: &str) -> Result<Vec<CertificateDer<'static>>, TlsError> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| TlsError::Pem(e.to_string()))
}

fn pem_key(pem: &str) -> Result<PrivateKeyDer<'static>, TlsError> {
    let mut reader = std::io::BufReader::new(pem.as_bytes());
    rustls_pemfile::private_key(&mut reader)
        .map_err(|e| TlsError::Pem(e.to_string()))?
        .ok_or_else(|| TlsError::Pem("no private key".into()))
}
