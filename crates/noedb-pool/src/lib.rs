//! gRPC connection pool with health checks and read/write routing (Phase 7, Week 50).

#![forbid(unsafe_code)]

mod config;
mod error;
mod pool;
mod router;

pub use config::PoolConfig;
pub use error::PoolError;
pub use pool::{GrpcPool, PooledClient, QueryRows};
pub use router::{LoadBalancer, Route};
