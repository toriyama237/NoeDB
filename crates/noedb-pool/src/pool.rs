//! Tokio gRPC channel pool.

use std::sync::Arc;
use std::time::Instant;

use noedb_grpc::{
    auth::inject_auth,
    tls::{client_tls_mtls, client_tls_one_way, peer_host_from_addr, MAX_GRPC_BYTES},
    SqlClient, SqlRequest, SqlResponse,
};
use noedb_raft::ClusterAuth;
use noedb_tls::DevCertPem;
use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

use crate::config::PoolConfig;
use crate::error::PoolError;
use crate::router::{LoadBalancer, Route};

/// Tabular result from `Execute`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryRows {
    /// Column names.
    pub columns: Vec<String>,
    /// Cell values per row.
    pub rows: Vec<Vec<String>>,
}

struct Inner {
    channel: Channel,
    last_ping: Instant,
}

/// Pooled gRPC SQL client (returned to pool on drop).
pub struct PooledClient {
    pool: Arc<GrpcPool>,
    inner: Option<Inner>,
    route: Route,
    _permit: OwnedSemaphorePermit,
}

/// Shared connection pool over one or more gRPC endpoints.
pub struct GrpcPool {
    config: PoolConfig,
    auth: ClusterAuth,
    certs: DevCertPem,
    mtls: bool,
    lb: LoadBalancer,
    semaphore: Arc<Semaphore>,
    idle: Mutex<Vec<Inner>>,
}

impl GrpcPool {
    /// Build a pool using dev TLS material and a load balancer.
    ///
    /// # Errors
    ///
    /// TLS configuration failures.
    pub fn new(
        config: PoolConfig,
        auth: ClusterAuth,
        certs: DevCertPem,
        mtls: bool,
        lb: LoadBalancer,
    ) -> Arc<Self> {
        let max_open = config.max_open;
        Arc::new(Self {
            config,
            auth,
            certs,
            mtls,
            lb,
            semaphore: Arc::new(Semaphore::new(max_open)),
            idle: Mutex::new(Vec::new()),
        })
    }

    /// Acquire a connection for `route` (may open a new channel).
    ///
    /// # Errors
    ///
    /// Pool exhausted or transport failures.
    pub async fn acquire(self: &Arc<Self>, route: Route) -> Result<PooledClient, PoolError> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| PoolError::Exhausted)?;

        if let Some(mut inner) = self.idle.lock().pop() {
            if inner.last_ping.elapsed() < self.config.health_interval {
                return Ok(PooledClient {
                    pool: Arc::clone(self),
                    inner: Some(inner),
                    route,
                    _permit: permit,
                });
            }
            if health_ping(&self.auth, &mut inner).await.is_ok() {
                return Ok(PooledClient {
                    pool: Arc::clone(self),
                    inner: Some(inner),
                    route,
                    _permit: permit,
                });
            }
        }

        let addr = self.lb.pick(route);
        let inner = connect_inner(addr, &self.certs, self.mtls).await?;
        Ok(PooledClient {
            pool: Arc::clone(self),
            inner: Some(inner),
            route,
            _permit: permit,
        })
    }

    /// One-shot SQL on the pool.
    ///
    /// # Errors
    ///
    /// See [`PooledClient::execute`].
    pub async fn execute(
        self: &Arc<Self>,
        route: Route,
        sql: &str,
    ) -> Result<QueryRows, PoolError> {
        let mut client = self.acquire(route).await?;
        client.execute(sql).await
    }

    fn release(&self, inner: Inner) {
        let mut idle = self.idle.lock();
        if idle.len() < self.config.min_idle {
            idle.push(inner);
        }
    }
}

impl PooledClient {
    /// Run SQL on this connection.
    ///
    /// # Errors
    ///
    /// RPC or application SQL errors.
    pub async fn execute(&mut self, sql: &str) -> Result<QueryRows, PoolError> {
        let inner = self.inner.as_mut().ok_or(PoolError::Exhausted)?;
        let mut client = SqlClient::new(inner.channel.clone())
            .max_decoding_message_size(MAX_GRPC_BYTES)
            .max_encoding_message_size(MAX_GRPC_BYTES);
        let req = inject_auth(
            Request::new(SqlRequest {
                query: sql.to_string(),
            }),
            &self.pool.auth,
        )?;
        let resp = client.execute(req).await?;
        inner.last_ping = Instant::now();
        parse_rows(resp.into_inner())
    }
}

impl Drop for PooledClient {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            self.pool.release(inner);
        }
    }
}

async fn connect_inner(addr: &str, certs: &DevCertPem, mtls: bool) -> Result<Inner, PoolError> {
    let host = peer_host_from_addr(addr);
    let tls = if mtls {
        client_tls_mtls(certs, host).map_err(|e| PoolError::Tls(e.to_string()))?
    } else {
        client_tls_one_way(certs, host).map_err(|e| PoolError::Tls(e.to_string()))?
    };
    let uri = if addr.starts_with("http") {
        addr.to_string()
    } else {
        format!("https://{addr}")
    };
    let channel = Endpoint::from_shared(uri)
        .map_err(|e| PoolError::Tls(e.to_string()))?
        .tls_config(tls)
        .map_err(|e| PoolError::Tls(e.to_string()))?
        .connect()
        .await?;
    Ok(Inner {
        channel,
        last_ping: Instant::now(),
    })
}

async fn health_ping(auth: &ClusterAuth, inner: &mut Inner) -> Result<(), PoolError> {
    let mut client = SqlClient::new(inner.channel.clone());
    let req = inject_auth(Request::new(noedb_grpc::PingRequest {}), auth)?;
    let _ = client.ping(req).await?;
    inner.last_ping = Instant::now();
    Ok(())
}

fn parse_rows(resp: SqlResponse) -> Result<QueryRows, PoolError> {
    match resp.body {
        Some(noedb_grpc::generated::sql_response::Body::Result(rs)) => Ok(QueryRows {
            columns: rs.columns,
            rows: rs.rows.into_iter().map(|r| r.cells).collect(),
        }),
        Some(noedb_grpc::generated::sql_response::Body::Error(e)) => Err(PoolError::Sql(e.message)),
        other => Err(PoolError::Sql(format!("unexpected response: {other:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::Route;

    #[test]
    fn high_concurrency_config_allows_1024() {
        let cfg = PoolConfig::high_concurrency();
        assert_eq!(cfg.max_open, 1024);
    }
}
