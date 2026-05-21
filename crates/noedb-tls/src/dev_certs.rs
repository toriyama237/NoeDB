//! Development self-signed certificates (rcgen).

use rcgen::{CertificateParams, DistinguishedName, DnType, KeyPair, SanType};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

use crate::error::TlsError;
use crate::identity::SpiffeId;

/// PEM-encoded dev PKI: CA, server, and mTLS client identity.
#[derive(Debug, Clone)]
pub struct DevCertPem {
    /// CA certificate PEM (trust anchor).
    pub ca_pem: String,
    /// Server certificate PEM.
    pub cert_pem: String,
    /// Server private key PEM.
    pub key_pem: String,
    /// Client certificate PEM (mTLS).
    pub client_cert_pem: String,
    /// Client private key PEM.
    pub client_key_pem: String,
    /// SPIFFE-style CN on the client cert.
    pub client_spiffe_cn: String,
}

impl DevCertPem {
    /// Generate localhost server + client `node_id` signed by a fresh dev CA.
    ///
    /// # Errors
    ///
    /// rcgen or key generation failures.
    pub fn generate_cluster(node_id: u64) -> Result<Self, TlsError> {
        let ca_key = KeyPair::generate().map_err(|e| TlsError::CertGen(e.to_string()))?;
        let server_key = KeyPair::generate().map_err(|e| TlsError::CertGen(e.to_string()))?;
        let client_key = KeyPair::generate().map_err(|e| TlsError::CertGen(e.to_string()))?;

        let mut ca_params = CertificateParams::default();
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let mut ca_dn = DistinguishedName::new();
        ca_dn.push(DnType::CommonName, "NoeDB Dev CA");
        ca_params.distinguished_name = ca_dn;
        let ca_cert = ca_params
            .self_signed(&ca_key)
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
            .signed_by(&server_key, &ca_cert, &ca_key)
            .map_err(|e| TlsError::CertGen(e.to_string()))?;

        let client_spiffe_cn = SpiffeId::cn(node_id);
        let mut client_params = CertificateParams::default();
        let mut client_dn = DistinguishedName::new();
        client_dn.push(DnType::CommonName, &client_spiffe_cn);
        client_params.distinguished_name = client_dn;
        let client_cert = client_params
            .signed_by(&client_key, &ca_cert, &ca_key)
            .map_err(|e| TlsError::CertGen(e.to_string()))?;

        Ok(Self {
            ca_pem: ca_cert.pem(),
            cert_pem: server_cert.pem(),
            key_pem: server_key.serialize_pem(),
            client_cert_pem: client_cert.pem(),
            client_key_pem: client_key.serialize_pem(),
            client_spiffe_cn,
        })
    }

    /// Back-compat: cluster bundle for node 1.
    ///
    /// # Errors
    ///
    /// See [`Self::generate_cluster`].
    pub fn generate_localhost() -> Result<Self, TlsError> {
        Self::generate_cluster(1)
    }

    /// Server cert chain + key for rustls.
    ///
    /// # Errors
    ///
    /// PEM or rustls parse errors.
    pub fn server_identity(&self) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TlsError> {
        let certs = pem_certs(&self.cert_pem)?;
        let key = pem_key(&self.key_pem)?;
        Ok((certs, key))
    }

    /// Client cert chain + key for mTLS.
    ///
    /// # Errors
    ///
    /// PEM or rustls parse errors.
    pub fn client_identity(&self) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), TlsError> {
        let certs = pem_certs(&self.client_cert_pem)?;
        let key = pem_key(&self.client_key_pem)?;
        Ok((certs, key))
    }

    /// Parse CA cert for trust stores.
    ///
    /// # Errors
    ///
    /// PEM parse errors.
    pub fn ca_cert(&self) -> Result<CertificateDer<'static>, TlsError> {
        let mut certs = pem_certs(&self.ca_pem)?;
        certs.pop().ok_or_else(|| TlsError::Pem("empty CA".into()))
    }

    /// Leaf server certificate bytes for pinning.
    ///
    /// # Errors
    ///
    /// PEM parse errors.
    pub fn server_leaf_der(&self) -> Result<CertificateDer<'static>, TlsError> {
        let mut certs = pem_certs(&self.cert_pem)?;
        certs.pop().ok_or_else(|| TlsError::Pem("empty server cert".into()))
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
