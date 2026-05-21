//! SPIFFE-style peer identities (Phase 1, Week 2).

/// SPIFFE URI prefix for NoeDB cluster members.
pub const SPIFFE_PREFIX: &str = "spiffe://noedb/cluster/node/";

/// Parsed SPIFFE-style node identity from a certificate CN.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpiffeId {
    /// Cluster node id (1-based Raft id).
    pub node_id: u64,
}

impl SpiffeId {
    /// Build SPIFFE CN string for a Raft [`node_id`](Self::node_id).
    #[must_use]
    pub fn cn(node_id: u64) -> String {
        format!("{SPIFFE_PREFIX}{node_id}")
    }

    /// Parse CN from a presented client certificate.
    ///
    /// # Errors
    ///
    /// CN does not match the expected SPIFFE layout.
    pub fn parse_cn(cn: &str) -> Result<Self, crate::TlsError> {
        let rest = cn
            .strip_prefix(SPIFFE_PREFIX)
            .ok_or_else(|| crate::TlsError::Identity(format!("bad spiffe cn: {cn}")))?;
        let node_id = rest
            .parse()
            .map_err(|_| crate::TlsError::Identity(format!("bad node id in cn: {cn}")))?;
        Ok(Self { node_id })
    }
}
