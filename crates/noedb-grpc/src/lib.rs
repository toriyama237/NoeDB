//! gRPC SQL API for NoeDB (Phase 1 Week 3).
//!
//! - Proto3 [`sql`](generated::noedb::v1) service
//! - mTLS via tonic + dev PKI from [`noedb_tls`]
//! - Cluster auth on `x-noedb-auth` metadata

#![forbid(unsafe_code)]
#![allow(clippy::all, clippy::nursery, unused_qualifications)]

pub mod auth;
pub mod tls;

pub use auth::{inject_auth, verify_auth, AUTH_METADATA};
pub use tls::{
    client_tls_mtls, client_tls_one_way, peer_host_from_addr, server_tls_mtls, server_tls_one_way,
    DEFAULT_GRPC_ADDR, MAX_GRPC_BYTES,
};

/// Generated protobuf + tonic stubs.
pub mod generated {
    #![allow(
        clippy::all,
        clippy::pedantic,
        unused_qualifications,
        missing_docs,
        unreachable_pub
    )]
    tonic::include_proto!("noedb.v1");
}

pub use generated::sql_client::SqlClient;
pub use generated::sql_server::{Sql, SqlServer};
pub use generated::{Empty, PingRequest, ResultSet, Row, SqlError, SqlRequest, SqlResponse};
