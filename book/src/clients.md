# Clients & connection pool

## Language drivers (gRPC + TLS)

| Language | Path | Package |
|----------|------|---------|
| Python | `clients/python` | `pip install -e .` |
| Go | `clients/go` | `github.com/toriyama237/noedb-go` |
| Node.js | `clients/nodejs` | `@noe/noedb-client` |

All clients send cluster auth on metadata key `x-noedb-auth` (hex of 32-byte token
derived from passphrase, default `noedb-dev`). Dev TLS files live under
`<data-dir>/tls/` (`ca.pem`, `client.pem`, `client-key.pem`) after
`noedb-cli --server` starts.

## Rust connection pool

`noedb-pool` provides:

- `GrpcPool` — bounded channels, idle reuse, periodic `Ping` health checks.
- `LoadBalancer` — `Route::Write` → leader, `Route::Read` → round-robin replicas.

```rust
let pool = GrpcPool::new(
    PoolConfig::high_concurrency(),
    ClusterAuth::from_passphrase("noedb-dev"),
    certs,
    true,
    LoadBalancer::new("127.0.0.1:5434", vec!["127.0.0.1:5435".into()]),
);
let rows = pool.execute(Route::Read, "SELECT 1").await?;
```

Cross-driver smoke: `scripts/cross_driver_contract.sh` (server must be running).
