# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | ✅        |
| < 1.0   | ❌        |

## Reporting a vulnerability

Email or open a **private** security advisory on GitHub. Do not file public issues for exploitable bugs.

## Phase 1 (v2 roadmap) — in progress

- **TLS 1.3** on wire protocol (`noedb-tls`, default on `noedb-cli --server`)
- Dev certificates are **self-signed** — not for production. Use proper PKI before exposing to a network.
- Cluster auth token (`ClusterAuth`) remains required inside TLS frames.

## Dependency audit

```bash
cargo audit
```

Run in CI before each release.

## Fuzzing (planned Week 6)

```bash
cargo fuzz run sql_parser
```
