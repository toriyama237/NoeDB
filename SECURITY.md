# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | Yes       |
| < 1.0   | No        |

## Reporting a vulnerability

Do **not** open public GitHub issues for exploitable bugs. Use a **private security advisory** on GitHub or contact the maintainers directly.

## Phase 1 — Security & Protocol (v2 roadmap)

| Layer | Status | Notes |
|-------|--------|-------|
| TLS 1.3 | Shipped | `noedb-tls`, default on `noedb-cli --server` |
| mTLS + SPIFFE | Shipped | Client cert CN `spiffe://noedb/cluster/node/{id}` |
| gRPC (tonic) | Shipped | Default `:5434`, `--legacy-tcp` for bincode |
| Prepared statements | Shipped | `PREPARE` / `EXECUTE`, typed bind — no string concat |
| Row Level Security | Shipped | `ENABLE ROW LEVEL SECURITY`, `CREATE POLICY`, `SET ROLE` |
| Audit log | Shipped | Append-only JSON lines under `data/audit/audit.log` |

Dev certificates are **self-signed** (`rcgen`). Use proper PKI before any production exposure.

Cluster auth (`ClusterAuth` / `x-noedb-auth` on gRPC) is required on every RPC.

## Dependency audit

Run before each release:

```bash
cargo install cargo-audit
cargo audit
```

CI should fail on unsound or critical advisories.

## Fuzzing

Parser fuzz target (Phase 1 Week 6):

```bash
cargo install cargo-fuzz
cargo fuzz run sql_parser -- -max_total_time=60
```

Lexer fuzz:

```bash
cargo fuzz run lexer -- -max_total_time=60
```

## Secure defaults

- Wire: mTLS + TLS 1.3 only (no SSLv3/TLS1.2 fallback in `noedb-tls`)
- Frames capped at **1 MiB** (gRPC + legacy TCP)
- SQL input capped at **64 KiB** per statement
- Prepared parameters bound as typed literals — SQL injection structurally blocked on the prepared path

## CVE policy

We aim for **zero known critical/high CVEs** in the dependency tree at release tags. Patch within 7 days for critical issues affecting default configurations.
