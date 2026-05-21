//! NoeDB interactive shell and optional TCP server (Phase 5).

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use noedb_engine::{DistributedEngine, EngineError, LocalEngine, QueryResult};
use noedb_protocol::{decode_request, encode_response, Request, Response};
use noedb_raft::ClusterAuth;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
    println!("NoeDB v1 — type SQL ending with ';', \\explain <sql>, or \\q to quit.");
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
        let sql = trimmed.trim_end_matches(';');
        match backend.execute(sql) {
            Ok(result) => print_result(&result),
            Err(e) => eprintln!("error: {e}"),
        }
    }
    Ok(())
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
        println!("noedb listening on {addr} (auth: noedb-dev passphrase)");
        loop {
            let (stream, peer) = listener.accept().await.map_err(|e| e.to_string())?;
            let auth = auth.clone();
            let engine = Arc::clone(&engine);
            tokio::spawn(async move {
                if let Err(e) = handle_client(stream, auth, engine).await {
                    eprintln!("client {peer}: {e}");
                }
            });
        }
    })
}

async fn handle_client(
    mut stream: TcpStream,
    auth: ClusterAuth,
    engine: Arc<Mutex<LocalEngine>>,
) -> Result<(), String> {
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
    println!("\\explain    show plan for SELECT");
    println!("SQL;        execute query (SELECT, CREATE INDEX)");
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

fn server_addr(args: &[String]) -> String {
    args.iter()
        .position(|a| a == "--listen")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "127.0.0.1:5433".to_string())
}
