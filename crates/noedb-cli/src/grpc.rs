//! gRPC SQL server and client (Phase 1 Week 3).

use std::sync::Arc;

use noedb_engine::{EngineError, LocalEngine, QueryResult};
use noedb_grpc::{
    auth::{inject_auth, verify_auth},
    tls::{
        client_tls_mtls, client_tls_one_way, peer_host_from_addr, server_tls_mtls,
        server_tls_one_way, MAX_GRPC_BYTES,
    },
    Empty, PingRequest, ResultSet, Row, Sql, SqlClient, SqlRequest, SqlResponse, SqlServer,
};
use noedb_raft::ClusterAuth;
use tokio::sync::Semaphore;
use tonic::transport::{Channel, Endpoint, Server};
use tonic::{Request, Response as GrpcResponse, Status};

const DEFAULT_MAX_INFLIGHT: usize = 512;

struct SqlServiceImpl {
    auth: ClusterAuth,
    engine: Arc<LocalEngine>,
    permits: Arc<Semaphore>,
}

impl SqlServiceImpl {
    fn new(auth: ClusterAuth, engine: Arc<LocalEngine>) -> Self {
        let max = std::env::var("NOEDB_GRPC_MAX_INFLIGHT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(DEFAULT_MAX_INFLIGHT);
        Self {
            auth,
            engine,
            permits: Arc::new(Semaphore::new(max)),
        }
    }
}

#[tonic::async_trait]
impl Sql for SqlServiceImpl {
    async fn execute(
        &self,
        request: Request<SqlRequest>,
    ) -> Result<GrpcResponse<SqlResponse>, Status> {
        verify_auth(&request, &self.auth)?;
        let _permit = self.permits.acquire().await.map_err(|_| {
            Status::resource_exhausted("gRPC in-flight limit — retry with backoff")
        })?;
        let query = request.into_inner().query;
        let engine = Arc::clone(&self.engine);
        let resp = tokio::task::spawn_blocking(move || dispatch_sql(&engine, &query))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(GrpcResponse::new(resp))
    }

    async fn explain(
        &self,
        request: Request<SqlRequest>,
    ) -> Result<GrpcResponse<SqlResponse>, Status> {
        verify_auth(&request, &self.auth)?;
        let _permit = self.permits.acquire().await.map_err(|_| {
            Status::resource_exhausted("gRPC in-flight limit — retry with backoff")
        })?;
        let query = request.into_inner().query;
        let engine = Arc::clone(&self.engine);
        let resp = tokio::task::spawn_blocking(move || match engine.explain(&query) {
            Ok(text) => SqlResponse {
                body: Some(noedb_grpc::generated::sql_response::Body::Explain(text)),
            },
            Err(e) => sql_error(&e),
        })
        .await
        .map_err(|e| Status::internal(e.to_string()))?;
        Ok(GrpcResponse::new(resp))
    }

    async fn ping(
        &self,
        request: Request<PingRequest>,
    ) -> Result<GrpcResponse<SqlResponse>, Status> {
        verify_auth(&request, &self.auth)?;
        let _ = request.into_inner();
        Ok(GrpcResponse::new(SqlResponse {
            body: Some(noedb_grpc::generated::sql_response::Body::Pong(Empty {})),
        }))
    }
}

fn dispatch_sql(eng: &LocalEngine, query: &str) -> SqlResponse {
    match eng.execute(query) {
        Ok(r) => result_set_response(r),
        Err(e) => sql_error(&e),
    }
}

fn result_set_response(r: QueryResult) -> SqlResponse {
    SqlResponse {
        body: Some(noedb_grpc::generated::sql_response::Body::Result(
            ResultSet {
                columns: r.columns,
                rows: r.rows.into_iter().map(|cells| Row { cells }).collect(),
            },
        )),
    }
}

fn sql_error(e: &EngineError) -> SqlResponse {
    SqlResponse {
        body: Some(noedb_grpc::generated::sql_response::Body::Error(
            noedb_grpc::SqlError {
                code: e.grpc_code(),
                message: e.to_string(),
            },
        )),
    }
}

/// Run gRPC server until process exit.
pub(crate) async fn run_grpc_server(
    addr: &str,
    auth: ClusterAuth,
    engine: Arc<LocalEngine>,
    mtls: bool,
    certs: &noedb_tls::DevCertPem,
) -> Result<(), String> {
    let tls = if mtls {
        server_tls_mtls(certs)?
    } else {
        server_tls_one_way(certs)?
    };
    let svc = SqlServer::new(SqlServiceImpl::new(auth, engine))
        .max_decoding_message_size(MAX_GRPC_BYTES)
        .max_encoding_message_size(MAX_GRPC_BYTES);

    let mode = if mtls { "mTLS 1.3" } else { "TLS 1.3" };
    println!("noedb gRPC on {addr} ({mode}, HTTP/2, max_inflight={DEFAULT_MAX_INFLIGHT})");
    println!("  legacy TCP: use --legacy-tcp on port 5433");

    Server::builder()
        .tls_config(tls)
        .map_err(|e| e.to_string())?
        .add_service(svc)
        .serve(
            addr.parse()
                .map_err(|e: std::net::AddrParseError| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())
}

/// Ping over gRPC (mTLS or one-way TLS).
pub(crate) async fn grpc_ping(
    addr: &str,
    auth: &ClusterAuth,
    mtls: bool,
    certs: &noedb_tls::DevCertPem,
) -> Result<(), String> {
    let host = peer_host_from_addr(addr);
    let tls = if mtls {
        client_tls_mtls(certs, host)?
    } else {
        client_tls_one_way(certs, host)?
    };
    let channel = connect(addr, tls).await?;
    let mut client = SqlClient::new(channel)
        .max_decoding_message_size(MAX_GRPC_BYTES)
        .max_encoding_message_size(MAX_GRPC_BYTES);
    let req = inject_auth(Request::new(PingRequest {}), auth).map_err(|e| e.to_string())?;
    let resp = client.ping(req).await.map_err(|e| e.to_string())?;
    match resp.into_inner().body {
        Some(noedb_grpc::generated::sql_response::Body::Pong(_)) => Ok(()),
        other => Err(format!("unexpected gRPC response: {other:?}")),
    }
}

async fn connect(addr: &str, tls: tonic::transport::ClientTlsConfig) -> Result<Channel, String> {
    let uri = if addr.starts_with("http") {
        addr.to_string()
    } else {
        format!("https://{addr}")
    };
    Endpoint::from_shared(uri)
        .map_err(|e| e.to_string())?
        .tls_config(tls)
        .map_err(|e| e.to_string())?
        .connect()
        .await
        .map_err(|e| e.to_string())
}
