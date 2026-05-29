# Post–v2.0.0 roadmap

The [52-week v2 sprint](sprint-plan-v2.md) is complete (`v2.0.0`). Possible follow-ups:

| Area | Idea | Notes |
|------|------|--------|
| Consensus | `madsim` fault injection | Deferred from Phase 4/6; simulator + proptest exist today |
| Benchmarks | YCSB workload A/B in CI smoke | Bench crate exists (`ycsb`); not gated in CI |
| Observability | OpenTelemetry exporter + Grafana JSON | Metrics + `tracing` spans in place |
| Distribution | Publish `noedb` to crates.io | Meta-crate API stabilization |
| Clients | PyPI / npm / pkg.go.dev releases | Sources under `clients/` |
| SQL | Full production DDL, follower reads on wire | `DistributedEngine` is in-process only for now |
| Docs | This site | Built by [`docs.yml`](../.github/workflows/docs.yml) → GitHub Pages |

Contributions welcome — pick an item and open an issue first if the scope is large.
