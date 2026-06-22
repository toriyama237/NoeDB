//! Phase 6 observability (Week 45): Prometheus metrics.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use noedb_engine::LocalEngine;
use noedb_metrics::spawn_prometheus_listener;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_engine() -> (Arc<LocalEngine>, std::path::PathBuf) {
    let n = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "noedb-phase6-metrics-{}-{}-{}",
        std::process::id(),
        n,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (LocalEngine::open(&dir).unwrap(), dir)
}

#[test]
fn metrics_increment_on_queries_and_cache() {
    let (eng, dir) = temp_engine();
    eng.execute("SELECT 1").unwrap();
    eng.execute("SELECT 1").unwrap();
    let text = eng.metrics().render_prometheus();
    assert!(text.contains("noedb_queries_total 2"));
    assert!(text.contains("noedb_cache_hits_total 1"));
    assert!(text.contains("noedb_cache_misses_total 1"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn http_metrics_endpoint_serves_prometheus() {
    let (eng, dir) = temp_engine();
    let metrics = eng.metrics();
    eng.execute("SELECT 42").unwrap();

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let _handle = spawn_prometheus_listener(addr, metrics).unwrap();
    std::thread::sleep(Duration::from_millis(50));

    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .write_all(b"GET /metrics HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut body = String::new();
    stream.read_to_string(&mut body).unwrap();
    assert!(body.contains("200"));
    assert!(body.contains("noedb_queries_total 1"));
    let _ = std::fs::remove_dir_all(dir);
}
