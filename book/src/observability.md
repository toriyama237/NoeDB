# Observability

## Metrics

`noedb-metrics` exposes Prometheus text on `GET /metrics`:

- `noedb_queries_total`, `noedb_query_errors_total`
- `noedb_query_duration_seconds_{sum,count,max}`
- `noedb_cache_hits_total`, `noedb_cache_misses_total`
- `noedb_raft_leader_id`, `noedb_raft_no_leader_total`

Enable with:

```bash
cargo run -p noedb-cli -- --metrics-listen 127.0.0.1:9090
```

## Tracing

SQL execution emits `tracing` span `noedb.sql.execute`.

Export spans to Jaeger / Grafana Tempo via OTLP:

```bash
cargo run -p noedb-cli --features otel -- \
  --otel-endpoint http://127.0.0.1:4317 \
  --server --data-dir /tmp/noedb-dev
```

Set `RUST_LOG=noedb_engine=debug` for verbose spans.

## Grafana

Import [`docs/grafana/noedb-overview.json`](../../docs/grafana/noedb-overview.json)
and point Prometheus at `http://127.0.0.1:9090/metrics` (with `--metrics-listen`).

## Reliability tests

- `cargo fuzz run protocol_wire`
- `phase6_chaos.rs` — leader crash + re-election RTO smoke
- `raft_proptest.rs` — random Raft proposals
