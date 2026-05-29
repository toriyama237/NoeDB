//! Pool errors.

use thiserror::Error;

/// Connection pool failure.
#[derive(Debug, Error)]
pub enum PoolError {
    /// Pool is at capacity.
    #[error("pool exhausted (max_open reached)")]
    Exhausted,
    /// gRPC transport failure.
    #[error("grpc: {0}")]
    Grpc(#[from] tonic::transport::Error),
    /// RPC status.
    #[error("rpc: {0}")]
    Rpc(#[from] tonic::Status),
    /// TLS / cert load failure.
    #[error("tls: {0}")]
    Tls(String),
    /// SQL returned application error.
    #[error("sql: {0}")]
    Sql(String),
}
