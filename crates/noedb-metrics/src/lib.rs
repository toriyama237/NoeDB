//! Counters, gauges, histograms, and Prometheus text exposition (Phase 6, Week 45).

#![forbid(unsafe_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Monotonically increasing counter.
#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    /// Increment by one.
    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// Add `n`.
    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    /// Current value.
    #[must_use]
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Point-in-time gauge (stored as integer; use for counts and flags).
#[derive(Debug, Default)]
pub struct Gauge(AtomicU64);

impl Gauge {
    /// Set absolute value.
    pub fn set(&self, v: u64) {
        self.0.store(v, Ordering::Relaxed);
    }

    /// Current value.
    #[must_use]
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Duration histogram (sum / count / max in nanoseconds).
#[derive(Debug, Default)]
pub struct Histogram {
    sum_ns: AtomicU64,
    count: AtomicU64,
    max_ns: AtomicU64,
}

impl Histogram {
    /// Record one sample.
    pub fn observe(&self, d: Duration) {
        let ns = u64::try_from(d.as_nanos()).unwrap_or(u64::MAX);
        self.sum_ns.fetch_add(ns, Ordering::Relaxed);
        self.count.fetch_add(1, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }

    /// Total observed count.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }

    /// Sum of samples in seconds (for Prometheus).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn sum_seconds(&self) -> f64 {
        let ns = self.sum_ns.load(Ordering::Relaxed);
        ns as f64 / 1_000_000_000.0
    }

    /// Max sample in seconds.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn max_seconds(&self) -> f64 {
        let ns = self.max_ns.load(Ordering::Relaxed);
        ns as f64 / 1_000_000_000.0
    }
}

/// Engine-wide metric registry (thread-safe, lock-free hot path).
#[derive(Debug, Default)]
pub struct Metrics {
    /// Completed SQL executions (success or error).
    pub queries_total: Counter,
    /// SQL executions that returned an error.
    pub query_errors_total: Counter,
    /// End-to-end query latency.
    pub query_duration_seconds: Histogram,
    /// Prepared-plan / result cache hits.
    pub cache_hits_total: Counter,
    /// Cache lookups that executed the query.
    pub cache_misses_total: Counter,
    /// MVCC versions removed by GC.
    pub gc_versions_pruned_total: Counter,
    /// Operations that failed because no Raft leader was elected.
    pub raft_no_leader_total: Counter,
    /// Current Raft leader node id (0 = unknown).
    pub raft_leader_id: Gauge,
}

impl Metrics {
    /// Shared handle for engines and HTTP scrapers.
    #[must_use]
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Record one SQL execution.
    pub fn record_query(&self, elapsed: Duration, ok: bool) {
        self.queries_total.inc();
        if !ok {
            self.query_errors_total.inc();
        }
        self.query_duration_seconds.observe(elapsed);
    }

    /// Record query-cache outcome.
    pub fn record_cache(&self, hit: bool) {
        if hit {
            self.cache_hits_total.inc();
        } else {
            self.cache_misses_total.inc();
        }
    }

    /// Record MVCC GC work.
    pub fn record_gc(&self, versions_pruned: usize) {
        if versions_pruned > 0 {
            self.gc_versions_pruned_total
                .add(u64::try_from(versions_pruned).unwrap_or(u64::MAX));
        }
    }

    /// Record missing Raft leader.
    pub fn record_no_leader(&self) {
        self.raft_no_leader_total.inc();
        self.raft_leader_id.set(0);
    }

    /// Update leader gauge after a successful leader lookup.
    pub fn set_raft_leader(&self, id: u64) {
        self.raft_leader_id.set(id);
    }

    /// Render Prometheus text exposition format 0.0.4.
    #[must_use]
    pub fn render_prometheus(&self) -> String {
        let mut out = String::with_capacity(1024);
        write_counter(
            &mut out,
            "noedb_queries_total",
            "SQL statements executed",
            &self.queries_total,
        );
        write_counter(
            &mut out,
            "noedb_query_errors_total",
            "SQL statements that failed",
            &self.query_errors_total,
        );
        write_histogram(
            &mut out,
            "noedb_query_duration_seconds",
            "SQL execution latency",
            &self.query_duration_seconds,
        );
        write_counter(
            &mut out,
            "noedb_cache_hits_total",
            "Query cache hits",
            &self.cache_hits_total,
        );
        write_counter(
            &mut out,
            "noedb_cache_misses_total",
            "Query cache misses",
            &self.cache_misses_total,
        );
        write_counter(
            &mut out,
            "noedb_gc_versions_pruned_total",
            "MVCC versions pruned by GC",
            &self.gc_versions_pruned_total,
        );
        write_counter(
            &mut out,
            "noedb_raft_no_leader_total",
            "Operations blocked with no Raft leader",
            &self.raft_no_leader_total,
        );
        write_gauge(
            &mut out,
            "noedb_raft_leader_id",
            "Current Raft leader node id (0 = unknown)",
            self.raft_leader_id.get(),
        );
        out
    }
}

fn write_counter(out: &mut String, name: &str, help: &str, c: &Counter) {
    use std::fmt::Write;
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} counter");
    let _ = writeln!(out, "{name} {}", c.get());
}

fn write_gauge(out: &mut String, name: &str, help: &str, v: u64) {
    use std::fmt::Write;
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} gauge");
    let _ = writeln!(out, "{name} {v}");
}

fn write_histogram(out: &mut String, name: &str, help: &str, h: &Histogram) {
    use std::fmt::Write;
    let _ = writeln!(out, "# HELP {name} {help}");
    let _ = writeln!(out, "# TYPE {name} histogram");
    let _ = writeln!(out, "{name}_sum {}", h.sum_seconds());
    let _ = writeln!(out, "{name}_count {}", h.count());
    let _ = writeln!(out, "{name}_max {}", h.max_seconds());
}

/// Spawn a background thread serving `GET /metrics` (Prometheus scrape).
///
/// # Errors
///
/// Bind or thread spawn failures.
pub fn spawn_prometheus_listener(
    addr: SocketAddr,
    metrics: Arc<Metrics>,
) -> std::io::Result<JoinHandle<()>> {
    let listener = TcpListener::bind(addr)?;
    listener.set_nonblocking(false)?;
    thread::Builder::new()
        .name("noedb-metrics".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = serve_http(&metrics, stream);
            }
        })
        .map_err(std::io::Error::other)
}

fn serve_http(metrics: &Metrics, mut stream: TcpStream) -> std::io::Result<()> {
    let mut buf = [0_u8; 512];
    let n = stream.read(&mut buf)?;
    let req = std::str::from_utf8(&buf[..n]).unwrap_or("");
    let path = req.lines().next().unwrap_or("");
    let (status, body) = if path.starts_with("GET /metrics") || path.starts_with("GET / ") {
        let body = metrics.render_prometheus();
        (200, body)
    } else {
        (404, "# not found\n".to_string())
    };
    let response = format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/plain; version=0.0.4; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn prometheus_text_contains_counters() {
        let m = Metrics::default();
        m.queries_total.inc();
        m.cache_hits_total.inc();
        m.query_duration_seconds.observe(Duration::from_millis(5));
        let text = m.render_prometheus();
        assert!(text.contains("noedb_queries_total 1"));
        assert!(text.contains("noedb_cache_hits_total 1"));
        assert!(text.contains("noedb_query_duration_seconds_count 1"));
    }
}
