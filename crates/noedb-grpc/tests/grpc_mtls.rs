#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    missing_docs
)]

use noedb_grpc::auth::{inject_auth, verify_auth};
use noedb_grpc::generated::sql_response::Body;
use noedb_grpc::tls::{client_tls_mtls, peer_host_from_addr, server_tls_mtls, MAX_GRPC_BYTES};
use noedb_grpc::{
    Empty, PingRequest, ResultSet, Row, Sql, SqlClient, SqlRequest, SqlResponse, SqlServer,
};
use noedb_raft::ClusterAuth;
use noedb_tls::DevCertPem;
use tonic::transport::Server;
use tonic::{Request, Response, Status};

struct EchoSql;

#[tonic::async_trait]
impl Sql for EchoSql {
    async fn execute(&self, request: Request<SqlRequest>) -> Result<Response<SqlResponse>, Status> {
        let auth = ClusterAuth::from_passphrase("noedb-dev");
        verify_auth(&request, &auth)?;
        Ok(Response::new(SqlResponse {
            body: Some(Body::Result(ResultSet {
                columns: vec!["x".into()],
                rows: vec![Row {
                    cells: vec![request.into_inner().query],
                }],
            })),
        }))
    }

    async fn explain(&self, _: Request<SqlRequest>) -> Result<Response<SqlResponse>, Status> {
        Err(Status::unimplemented("explain"))
    }

    async fn ping(&self, request: Request<PingRequest>) -> Result<Response<SqlResponse>, Status> {
        let auth = ClusterAuth::from_passphrase("noedb-dev");
        verify_auth(&request, &auth)?;
        Ok(Response::new(SqlResponse {
            body: Some(Body::Pong(Empty {})),
        }))
    }
}

#[tokio::test]
async fn mtls_grpc_ping_and_execute() {
    let certs = DevCertPem::generate_cluster(1).unwrap();
    let auth = ClusterAuth::from_passphrase("noedb-dev");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let https_addr = format!("127.0.0.1:{port}");

    let server_tls = server_tls_mtls(&certs).unwrap();
    let client_tls = client_tls_mtls(&certs, peer_host_from_addr(&https_addr)).unwrap();
    let svc = SqlServer::new(EchoSql)
        .max_decoding_message_size(MAX_GRPC_BYTES)
        .max_encoding_message_size(MAX_GRPC_BYTES);

    let serve_target = https_addr.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        Server::builder()
            .tls_config(server_tls)
            .unwrap()
            .add_service(svc)
            .serve_with_shutdown(serve_target.parse().unwrap(), async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let channel = tonic::transport::Endpoint::from_shared(format!("https://{https_addr}"))
        .unwrap()
        .tls_config(client_tls)
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut client = SqlClient::new(channel);

    let ping = inject_auth(Request::new(PingRequest {}), &auth).unwrap();
    let pong = tokio::time::timeout(std::time::Duration::from_secs(5), client.ping(ping))
        .await
        .expect("grpc ping timed out")
        .unwrap()
        .into_inner();
    assert!(matches!(pong.body, Some(Body::Pong(_))));

    let sql = inject_auth(
        Request::new(SqlRequest {
            query: "SELECT 1".into(),
        }),
        &auth,
    )
    .unwrap();
    let out = client.execute(sql).await.unwrap().into_inner();
    if let Some(Body::Result(rs)) = out.body {
        assert_eq!(rs.rows[0].cells[0], "SELECT 1");
    } else {
        panic!("expected result");
    }

    let _ = shutdown_tx.send(());
    let _ = server.await;
}
