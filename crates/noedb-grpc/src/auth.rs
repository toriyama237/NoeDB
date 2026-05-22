//! Cluster auth on gRPC metadata (`x-noedb-auth`).

use std::sync::LazyLock;

use noedb_raft::ClusterAuth;
use tonic::metadata::{MetadataKey, MetadataValue};
use tonic::{Request, Status};

/// Metadata key carrying the 32-byte cluster token (hex).
pub const AUTH_METADATA: &str = "x-noedb-auth";

static AUTH_KEY: LazyLock<MetadataKey<tonic::metadata::Ascii>> =
    LazyLock::new(|| MetadataKey::from_static(AUTH_METADATA));

/// Attach cluster auth to an outbound request.
///
/// # Errors
///
/// Invalid metadata encoding.
pub fn inject_auth<T>(mut req: Request<T>, auth: &ClusterAuth) -> Result<Request<T>, Status> {
    let value = auth_metadata_value(auth)?;
    req.metadata_mut().insert(AUTH_KEY.clone(), value);
    Ok(req)
}

/// Verify inbound metadata against the expected cluster token.
///
/// # Errors
///
/// Missing, malformed, or wrong auth.
pub fn verify_auth<T>(req: &Request<T>, expected: &ClusterAuth) -> Result<(), Status> {
    let meta = req
        .metadata()
        .get(AUTH_METADATA)
        .ok_or_else(|| Status::unauthenticated("missing x-noedb-auth"))?;
    let bytes = meta
        .to_str()
        .map_err(|_| Status::unauthenticated("invalid x-noedb-auth"))?;
    let raw =
        hex::decode(bytes).map_err(|_| Status::unauthenticated("invalid x-noedb-auth hex"))?;
    if raw.len() != 32 {
        return Err(Status::unauthenticated("x-noedb-auth must be 32 bytes"));
    }
    let mut token = [0u8; 32];
    token.copy_from_slice(&raw);
    let got = ClusterAuth(token);
    if !expected.verify(&got) {
        return Err(Status::unauthenticated("cluster auth mismatch"));
    }
    Ok(())
}

fn auth_metadata_value(
    auth: &ClusterAuth,
) -> Result<MetadataValue<tonic::metadata::Ascii>, Status> {
    let hex = hex::encode(auth.0);
    hex.parse()
        .map_err(|_| Status::internal("auth metadata encode failed"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn inject_and_verify_roundtrip() {
        let auth = ClusterAuth::from_passphrase("noedb-dev");
        let req = Request::new(());
        let req = inject_auth(req, &auth).unwrap();
        verify_auth(&req, &auth).unwrap();
    }

    #[test]
    fn rejects_wrong_passphrase() {
        let a = ClusterAuth::from_passphrase("a");
        let b = ClusterAuth::from_passphrase("b");
        let req = inject_auth(Request::new(()), &a).unwrap();
        assert!(verify_auth(&req, &b).is_err());
    }
}
