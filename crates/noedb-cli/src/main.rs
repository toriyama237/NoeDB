//! NoeDB interactive shell and optional TCP server (Phase 5 + Phase 1 TLS).

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod tls;

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use noedb_engine::{DistributedEngine, EngineError, LocalEngine, QueryResult};
use noedb_protocol::{decode_request, encode_response, Request, Response};
use noedb_raft::ClusterAuth;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;

enum Backend {
    Local(LocalEngine),
    Distributed(DistributedEngine),
}

impl Backend {
    fn execute(&mut self, sql: &str) -> Result<QueryResult, EngineError> {
        match self {
            Self::Local(e) => e.execute(sql),
            Self::Distributed(e) => e.execute(sql),
        }
    }

    fn explain(&mut self, sql: &str) -> Result<String, EngineError> {
        match self {
            Self::Local(e) => e.explain(sql),
            Self::Distributed(e) => e.explain(sql),
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--server") {
        if let Err(e) = run_server(&args) {
            eprintln!("noedb server error: {e}");
            std::process::exit(1);
        }
        return;
    }
    if args.iter().any(|a| a == "--ping") {
        if let Err(e) = run_ping(&args) {
            eprintln!("noedb ping error: {e}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = run_repl(&args) {
        eprintln!("noedb error: {e}");
        std::process::exit(1);
    }
}

fn run_repl(args: &[String]) -> Result<(), String> {
    let distributed = args.iter().any(|a| a == "--cluster");
    let mut backend = if distributed {
        Backend::Distributed(
            DistributedEngine::new_voters(3).map_err(|e| e.to_string())?,
        )
    } else {
        let dir = data_dir(args);
        Backend::Local(LocalEngine::open(&dir).map_err(|e| e.to_string())?)
    };
    if matches!(backend, Backend::Distributed(_)) {
        if let Backend::Distributed(ref mut e) = backend {
            e.tick(80).map_err(|e| e.to_string())?;
        }
    }
    println!("NoeDB v1.0 — interactive SQL shell (not your shell — type SQL here).");
    println!("  One statement per line, or several separated by ';'");
    println!("  Examples:  SELECT 1;");
    println!("             \\explain SELECT name FROM users WHERE id = '1'");
    println!("             \\q");
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        print!("noedb> ");
        io::stdout().flush().map_err(|e| e.to_string())?;
        line.clear();
        if stdin.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#')
            || trimmed.starts_with("cargo ")
            || trimmed.starts_with("git ")
        {
            eprintln!("hint: run shell commands in another terminal; this is the SQL REPL.");
            continue;
        }
        if trimmed == "\\q" || trimmed.eq_ignore_ascii_case("quit") {
            break;
        }
        if trimmed == "\\help" {
            print_help();
            continue;
        }
        if let Some(sql) = trimmed.strip_prefix("\\explain ") {
            match backend.explain(sql) {
                Ok(text) => println!("{text}"),
                Err(e) => eprintln!("error: {e}"),
            }
            continue;
        }
        for sql in split_statements(trimmed) {
            match backend.execute(&sql) {
                Ok(result) => print_result(&result),
                Err(e) => eprintln!("error: {e}"),
            }
        }
    }
    Ok(())
}

/// Split on `;` into non-empty statements (REPL convenience; not string-aware).
fn split_statements(line: &str) -> Vec<String> {
    line.split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn use_tls(args: &[String]) -> bool {
    !args.iter().any(|a| a == "--no-tls")
}

fn use_mtls(args: &[String]) -> bool {
    use_tls(args) && !args.iter().any(|a| a == "--no-mtls")
}

fn node_id_arg(args: &[String]) -> u64 {
    args.iter()
        .position(|a| a == "--node-id")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

fn run_server(args: &[String]) -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let addr = server_addr(args);
        let auth = ClusterAuth::from_passphrase("noedb-dev");
        let dir = data_dir(args);
        let engine = Arc::new(Mutex::new(
            LocalEngine::open(&dir).map_err(|e| e.to_string())?,
        ));
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("bind {addr}: {e}"))?;

        if use_mtls(args) {
            let node_id = node_id_arg(args);
            let certs = tls::load_or_create_dev_certs(&dir, node_id)?;
            let acceptor = tls::reloading_acceptor(&certs)?;
            println!("noedb listening on {addr} (mTLS 1.3, SPIFFE cn: {})", certs.client_spiffe_cn);
            println!("  CA: {}", tls::ca_path(&dir).display());
            loop {
                let (tcp, peer) = listener.accept().await.map_err(|e| e.to_string())?;
                let auth = auth.clone();
                let engine = Arc::clone(&engine);
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    match acceptor.accept(tcp).await {
                        Ok(stream) => {
                            if let Err(e) = handle_connection(stream, auth, engine).await {
                                eprintln!("client {peer}: {e}");
                            }
                        }
                        Err(e) => eprintln!("mtls handshake {peer}: {e}"),
                    }
                });
            }
        } else if use_tls(args) {
            let certs = tls::load_or_create_dev_certs(&dir, 1)?;
            let acceptor = tls::server_acceptor(&certs)?;
            println!("noedb listening on {addr} (TLS 1.3 one-way, auth: noedb-dev)");
            println!("  CA: {}", tls::ca_path(&dir).display());
            loop {
                let (tcp, peer) = listener.accept().await.map_err(|e| e.to_string())?;
                let auth = auth.clone();
                let engine = Arc::clone(&engine);
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    match acceptor.accept(tcp).await {
                        Ok(stream) => {
                            if let Err(e) = handle_connection(stream, auth, engine).await {
                                eprintln!("client {peer}: {e}");
                            }
                        }
                        Err(e) => eprintln!("tls handshake {peer}: {e}"),
                    }
                });
            }
        } else {
            println!("noedb listening on {addr} (plain TCP — dev only, use TLS in production)");
            loop {
                let (stream, peer) = listener.accept().await.map_err(|e| e.to_string())?;
                let auth = auth.clone();
                let engine = Arc::clone(&engine);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, auth, engine).await {
                        eprintln!("client {peer}: {e}");
                    }
                });
            }
        }
    })
}

fn run_ping(args: &[String]) -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let addr = server_addr(args);
        let dir = data_dir(args);
        let auth = ClusterAuth::from_passphrase("noedb-dev");

        if use_mtls(args) {
            let certs = tls::load_or_create_dev_certs(&dir, node_id_arg(args))?;
            let connector = if args.iter().any(|a| a == "--pin-server") {
                tls::client_connector_pinned(&certs)?
            } else {
                tls::client_connector(&certs)?
            };
            let tcp = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| e.to_string())?;
            let server_name = localhost_name()?;
            let mut stream = connector
                .connect(server_name, tcp)
                .await
                .map_err(|e| e.to_string())?;
            ping_over_connection(&mut stream, &auth).await?;
            println!("pong (mTLS 1.3)");
        } else if use_tls(args) {
            let certs = tls::load_or_create_dev_certs(&dir, 1)?;
            let connector = noedb_tls::client_connector_dev(&certs).map_err(|e| e.to_string())?;
            let tcp = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| e.to_string())?;
            let server_name = localhost_name()?;
            let mut stream = connector
                .connect(server_name, tcp)
                .await
                .map_err(|e| e.to_string())?;
            ping_over_connection(&mut stream, &auth).await?;
            println!("pong (TLS 1.3)");
        } else {
            let mut stream = tokio::net::TcpStream::connect(&addr)
                .await
                .map_err(|e| e.to_string())?;
            ping_over_connection(&mut stream, &auth).await?;
            println!("pong (plain TCP)");
        }
        Ok(())
    })
}

async fn ping_over_connection<S>(stream: &mut S, auth: &ClusterAuth) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use noedb_protocol::{encode_request, Request};
    let frame = encode_request(auth, &Request::Ping).map_err(|e| e.to_string())?;
    let len = u32::try_from(frame.len()).map_err(|_| "frame too large".to_string())?;
    stream
        .write_all(&len.to_le_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stream
        .write_all(&frame)
        .await
        .map_err(|e| e.to_string())?;
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stream
        .read_exact(&mut buf)
        .await
        .map_err(|e| e.to_string())?;
    let resp = noedb_protocol::decode_response(auth, &buf).map_err(|e| e.to_string())?;
    match resp {
        Response::Pong => Ok(()),
        other => Err(format!("unexpected response: {other:?}")),
    }
}

async fn handle_connection<S>(
    mut stream: S,
    auth: ClusterAuth,
    engine: Arc<Mutex<LocalEngine>>,
) -> Result<(), String>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| e.to_string())?;
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > noedb_raft::MAX_FRAME_BYTES {
        return Err("frame too large".into());
    }
    let mut frame = vec![0u8; len];
    stream
        .read_exact(&mut frame)
        .await
        .map_err(|e| e.to_string())?;
    let req = decode_request(&auth, &frame).map_err(|e| e.to_string())?;
    let resp = {
        let mut eng = engine.lock().map_err(|e| e.to_string())?;
        dispatch(&mut eng, req)
    };
    let out = encode_response(&auth, &resp).map_err(|e| e.to_string())?;
    let len = u32::try_from(out.len()).map_err(|_| "response too large".to_string())?;
    stream
        .write_all(&len.to_le_bytes())
        .await
        .map_err(|e| e.to_string())?;
    stream
        .write_all(&out)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn dispatch(eng: &mut LocalEngine, req: Request) -> Response {
    match req {
        Request::Ping => Response::Pong,
        Request::Explain { query } => match eng.explain(&query) {
            Ok(text) => Response::Explain { text },
            Err(e) => err_response(&e),
        },
        Request::Sql { query } => match eng.execute(&query) {
            Ok(r) => Response::Ok {
                columns: r.columns,
                rows: r.rows,
            },
            Err(e) => err_response(&e),
        },
    }
}

fn err_response(e: &EngineError) -> Response {
    Response::Error {
        code: 1,
        message: e.to_string(),
    }
}

fn print_result(r: &QueryResult) {
    if r.columns.is_empty() && r.rows.is_empty() {
        println!("OK");
        return;
    }
    println!("{}", r.columns.join("\t"));
    for row in &r.rows {
        println!("{}", row.join("\t"));
    }
}

fn print_help() {
    println!("\\q          quit");
    println!("\\help       this message");
    println!("\\explain    show plan for SELECT (one statement)");
    println!("SELECT ...;  one query per line, or use ';' between statements");
    println!("Server: cargo run -p noedb-cli -- --server [--listen HOST:PORT]");
    println!("Ping:   cargo run -p noedb-cli -- --ping   (TLS client, needs running server)");
}

fn data_dir(args: &[String]) -> PathBuf {
    args.iter()
        .position(|a| a == "--data")
        .and_then(|i| args.get(i + 1))
        .map_or_else(
            || std::env::temp_dir().join("noedb-data"),
            PathBuf::from,
        )
}

fn localhost_name() -> Result<rustls::pki_types::ServerName<'static>, String> {
    "localhost"
        .try_into()
        .map_err(|e: rustls::pki_types::InvalidDnsNameError| e.to_string())
}

fn server_addr(args: &[String]) -> String {
    args.iter()
        .position(|a| a == "--listen")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:5433".to_string())
}
