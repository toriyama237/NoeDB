<div align="center">

<p align="center">
  <img src="docs/assets/noedb-studio.png" alt="NoeDB Studio — distributed SQL engine CLI" width="920" />
</p>

# NoeDB

**A distributed SQL database engine, written from scratch in Rust.**

Every layer in-repo and auditable · Zero mandatory cloud dependency · Security by default

[![CI](https://github.com/toriyama237/NoeDB/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/toriyama237/NoeDB/actions/workflows/ci.yml)
[![Security audit](https://github.com/toriyama237/NoeDB/actions/workflows/audit.yml/badge.svg?branch=main)](https://github.com/toriyama237/NoeDB/actions/workflows/audit.yml)
[![CodeQL](https://github.com/toriyama237/NoeDB/actions/workflows/codeql.yml/badge.svg?branch=main)](https://github.com/toriyama237/NoeDB/actions/workflows/codeql.yml)
[![Rust stable](https://img.shields.io/badge/rust-stable-orange.svg?logo=rust)](https://www.rust-lang.org)
[![MSRV 1.88](https://img.shields.io/badge/MSRV-1.88-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](Cargo.toml)

[Status](#status--read-this-first) · [Quickstart](#quickstart) · [Architecture](#architecture) ·
[Security](#security-model) · [Benchmarks](#measured-not-claimed) · [Docs](https://toriyama237.github.io/NoeDB/) · [Contributing](#contributing)

</div>

---

## What NoeDB is

NoeDB is a complete SQL database engine built from first principles: lexer,
parser, cost-based planner, Volcano executor, MVCC transactions, LSM storage
with WAL, and Raft consensus — **every layer written in this repository**
(no `sqlx`, no `sled`, no embedded C). The goal is an engine you can read,
audit, and embed end to end:

| Property | How |
|---|---|
| **Auditability** | 100 % Rust, `unsafe` forbidden workspace-wide, ~40 k lines you can actually read |
| **Self-contained** | Runs fully on-premises or air-gapped; no telemetry, no license server, no phone-home |
| **Security by default** | mTLS + SPIFFE identity, RBAC + row-level security, at-rest encryption, hash-chained audit log, rate limiting |
| **Durability** | WAL-first LSM storage with CRC-32C on every frame, MVCC snapshots, Raft replication |
| **Traceability** | Conventional-commit history, `--no-ff` feature branches, CHANGELOG under Keep-a-Changelog |

## Status — read this first

NoeDB is a serious engineering project, **not a production-ready database**.
An honest map of where it stands:

- **Works and is tested**: the full SQL path (parse → plan → optimize →
  execute), MVCC snapshot transactions, WAL crash recovery, packed-row LSM
  storage with bounded scans, secondary-index maintenance, RBAC/RLS, the
  security hardening listed below — validated by 340+ tests and a 100 k-row
  end-to-end audit workload.
- **Works within limits**: Raft (election, replication, membership,
  snapshots) is validated with an in-process 3-node simulator and TCP+mTLS
  transport — it has **not** been through Jepsen-style network fault
  injection. Compaction covers L0→L1 only. MVCC garbage collection reclaims
  the active memtable, not flushed SSTs.
- **Does not exist yet**: online backup / point-in-time recovery,
  multi-region deployment tooling, a stabilized wire protocol, and the long
  tail of the SQL surface. One maintainer, no support contract.

If your data matters, run PostgreSQL. If you want a readable, tested,
from-scratch engine to study, extend, or embed in workloads that fit the
envelope above, that is exactly what NoeDB is for.

## Feature matrix

| Domain | Capabilities |
|---|---|
| **SQL** | `SELECT` / `INSERT` / `UPDATE` / `DELETE`, `INNER` / `LEFT JOIN`, `GROUP BY` / `HAVING`, window functions, CTEs (`WITH RECURSIVE`), subqueries (`IN` / `EXISTS`, correlated), set ops, `CAST`, `EXPLAIN`, `ANALYZE TABLE`, UTF-8 literals |
| **Constraints** | `PRIMARY KEY`, `UNIQUE`, `NOT NULL`, typed columns — enforced atomically before any storage write |
| **Storage** | LSM-tree: WAL segments, MemTable, SSTables (Bloom filters, 4 KiB blocks), leveled compaction, LZ4, CRC-32C |
| **Transactions** | MVCC snapshots, transactional SQL DML, garbage collection safe for latest versions |
| **Distribution** | Raft consensus (election, replication, joint-config membership, snapshots), 3-node in-process simulator, TCP + mTLS transport |
| **Query engine** | Volcano executors, cost-based optimizer (stats via `ANALYZE`), hash/merge/nested-loop joins, hash semi-joins for decorrelated subqueries, predicate & projection pushdown, partial top-k sort, SIMD predicate paths, Rayon parallel scans |
| **Security** | See [Security model](#security-model) |
| **Observability** | Prometheus `/metrics`, Grafana starter dashboard, structured audit export (SIEM-ready TSV) |
| **Ecosystem** | gRPC protocol + connection pool, Python / Go / Node.js clients, interactive REPL (`noedb-cli`), vector K-NN (HNSW) |

## Quickstart

```bash
git clone https://github.com/toriyama237/NoeDB.git
cd NoeDB
cargo test --workspace          # full validation suite
cargo run -p noedb-cli          # interactive REPL
```

Server mode (gRPC + TLS, Prometheus metrics):

```bash
cargo run -p noedb-cli -- --server --data-dir /var/lib/noedb \
    --metrics-listen 127.0.0.1:9090
```

Embedded, as a library:

```rust
use noedb::engine::LocalEngine;

let eng = LocalEngine::open("/var/lib/noedb")?;
eng.execute("CREATE TABLE accounts (id INT PRIMARY KEY, iban TEXT UNIQUE NOT NULL, balance INT NOT NULL)")?;
eng.execute("INSERT INTO accounts VALUES (1, 'FR7630001007941234567890185', 1000)")?;
let rows = eng.execute("SELECT iban, balance FROM accounts WHERE balance >= 500")?;
# Ok::<_, noedb::engine::EngineError>(())
```

Distributed (3-node Raft):

```rust
use noedb::engine::DistributedEngine;

let cluster = DistributedEngine::new_voters(3)?;
cluster.tick(80)?;
cluster.execute("CREATE TABLE t (id INT PRIMARY KEY)")?;
# Ok::<_, noedb::engine::EngineError>(())
```

## Architecture

```text
   ┌──────────┐   ┌──────────┐   ┌──────────────┐   ┌────────────┐   ┌───────────┐
   │   SQL    │   │  Lexer   │   │   Parser     │   │   Query    │   │   Raft    │
   │   text   │──▶│ (tokens) │──▶│    (AST)     │──▶│  Planner   │──▶│ consensus │
   └──────────┘   └──────────┘   └──────────────┘   └────────────┘   └─────┬─────┘
                                                                           │
                                                            ┌──────────────▼──────────────┐
                                                            │   LSM Storage Engine        │
                                                            │  (MemTable ─▶ WAL ─▶ SST)   │
                                                            └─────────────────────────────┘
```

One Cargo crate per box; the dependency graph is enforced by the workspace
(the lexer cannot depend on the planner, the planner cannot depend on Raft):

```text
crates/
├── noedb/            # meta-crate — the public, user-facing API
├── noedb-lexer/      # zero-copy tokenizer, byte-precise spans
├── noedb-ast/        # typed AST, SQL Display round-trip
├── noedb-parser/     # recursive descent + Pratt precedence
├── noedb-planner/    # logical/physical plans, cost-based optimizer, executors
├── noedb-storage/    # LSM-tree: WAL, MemTable, SSTables, compaction, MVCC
├── noedb-txn/        # transaction control
├── noedb-raft/       # Raft consensus core + transport
├── noedb-engine/     # SQL → security → planner → Raft → LSM orchestration
├── noedb-protocol/   # authenticated framed RPC
├── noedb-tls/        # mTLS, SPIFFE identity
├── noedb-grpc/       # gRPC service + auth guard
├── noedb-pool/       # client connection pool, health checks
├── noedb-metrics/    # Prometheus registry
└── noedb-cli/        # REPL + server binary
```

Full design docs: [mdBook](https://toriyama237.github.io/NoeDB/) ·
[`book/src/`](book/src/) (architecture, storage, Raft, query engine,
banking tutorial, observability).

## Security model

Hostile input is the default assumption. Defenses are enforced at the query
boundary, not bolted on:

| Threat | Defense | Where |
|--------|---------|-------|
| SQL injection via stacked queries | multi-statement batches rejected pre-parse | `security.rs` |
| Privilege abuse | DDL / RLS / policy changes gated to admin roles; `SET ROLE` escalation blocked | `security.rs` |
| Audit tampering | append-only log, SHA-256 hash chain, `--audit-verify` | `audit.rs` |
| Credential leakage in logs | secret literals masked before audit write | `redact.rs` |
| Query-flood / DoS | per-session token-bucket rate limiter + statement deadlines | `ratelimit.rs` |
| Data theft from disk/backups | at-rest encryption (XChaCha20-Poly1305, per-record nonce, cell-bound AAD) | `crypto.rs` |
| Brute-forced cluster auth | per-IP failure tracking with temporary ban | `noedb-grpc/auth_guard.rs` |
| Impersonation on the wire | mTLS with mandatory SPIFFE CN enforcement | `noedb-tls/spiffe.rs` |

```bash
# Verify the audit chain, then export for a SIEM
noedb --data /var/lib/noedb --audit-verify
noedb --data /var/lib/noedb --audit-export > audit.tsv

# At-rest encryption and per-session limits
export NOEDB_DATA_KEY="$(openssl rand -hex 32)"
export NOEDB_QPS=2000 NOEDB_STMT_TIMEOUT_MS=5000
```

Vulnerability reports: [private disclosure](https://github.com/toriyama237/NoeDB/security/advisories/new)
— policy in [SECURITY.md](./SECURITY.md).

## Measured, not claimed

NoeDB ships an end-to-end audit binary that simulates a mid-size payroll
system — **80 agencies, 22 departments, 50 009 employees, 50 009 payslips** —
and runs 20 business SQL benchmarks plus constraint checks:

```bash
cargo run --release -p noedb-engine --example national_hr_audit
```

Latest run (v2.4, release build, commodity hardware):

| Metric | Result |
|---|---|
| Bulk load (100 k rows, packed-row layout) | **1.8 s** (~60 k rows/s) |
| `COUNT(*)` over 50 k rows | **25 ms** (streaming aggregate; 250 ms in v2.3, 1.2 s in v2.2) |
| Point read `WHERE pk = X` over 50 k rows | **< 1 ms** (`PkLookup`; 220 ms in v2.3) |
| 20 business queries (3-way joins, aggregates, CTEs, subqueries) | **22 / 22 PASS** |
| Correlated `NOT EXISTS` on 50 k × 50 k | **0.6 s** (was 329 s in v2.0) |
| Full workspace test suite | **350+ tests, 0 failures** |

For calibration: SQLite answers the same `COUNT(*)` in single-digit
milliseconds — NoeDB is now within one order of magnitude on aggregates and
at parity on primary-key point reads. Multi-way joins and `ORDER BY` over
full tables still materialize rows as `Vec<(String, Value)>`; typed
columnar batches for those operators are the next lever. The numbers above
are honest, reproducible on your machine
(`cargo run --release -p noedb-engine --example bench_queries`), and
improving release over release.

Micro-benchmarks (Criterion): `cargo bench -p noedb-lexer --bench lexer`
(~1 M tokens / 21 ms), `cargo bench -p noedb-storage --bench lsm`,
`cargo bench -p noedb-engine --bench tpch_lite` (TPC-H lite analytical suite).

## Engineering standards

Every merge to `main` satisfies:

```bash
cargo fmt --all -- --check                                # formatting
cargo clippy --workspace --all-targets -- -D warnings     # zero warnings, pedantic on
cargo test --workspace                                    # full suite
cargo doc --no-deps                                       # documented public API
```

- `unsafe_code = "forbid"` — workspace-wide, no exceptions.
- `missing_docs = "warn"` — public API is documented.
- Supply chain: `cargo-deny` (licenses, advisories), `cargo audit`, CodeQL, typos check in CI.
- Git: feature branches, [Conventional Commits](https://www.conventionalcommits.org/),
  `--no-ff` merges — see [`docs/git-workflow.md`](docs/git-workflow.md).
- Releases: [SemVer](https://semver.org), [Keep a Changelog](https://keepachangelog.com) — see [CHANGELOG.md](CHANGELOG.md).

## Roadmap

| Milestone | Theme | Status |
|---|---|---|
| v0.x – v1.x | Lexer → parser → LSM → planner → Raft → engine | ✅ shipped |
| **v2.0** | Query engine v2, observability, clients, docs | ✅ shipped |
| **v2.1** | Planner performance (hash semi-join, top-k), UTF-8 SQL, audit tooling | ✅ shipped |
| **v2.2** | Bounded range scans, secondary-index DML maintenance, statement cache | ✅ shipped |
| **v2.3** | Packed row layout (one record per row), LSM read-order & tombstone fixes, realistic join costing | ✅ shipped |
| **v2.4** | Streaming aggregates, `PkLookup` point reads, key-only scans, `DROP TABLE` data purge | ✅ shipped |
| v2.5 | Typed columnar batches for joins & sorts, aggregate pushdown | 🔜 |
| v3.0 | Online backup/restore, point-in-time recovery, network fault-injection testing for Raft | planned |

History: [`docs/sprint-plan-v2.md`](docs/sprint-plan-v2.md) ·
[`docs/post-v2-roadmap.md`](docs/post-v2-roadmap.md).

## References

The design stands on published, peer-reviewed foundations:

- Ongaro & Ousterhout (2014), *In Search of an Understandable Consensus Algorithm (Raft)*
- O'Neil et al. (1996), *The Log-Structured Merge-Tree (LSM-Tree)*
- Graefe (1994), *Volcano — An Extensible and Parallel Query Evaluation System*
- Bloom (1970), *Space/Time Trade-offs in Hash Coding with Allowable Errors*
- Selinger et al. (1979), *Access Path Selection in a Relational Database Management System*

## Contributing

Issues, ideas, and PRs are welcome — see [CONTRIBUTING.md](./CONTRIBUTING.md).
Start with [`good first issue`](https://github.com/toriyama237/NoeDB/labels/good%20first%20issue).

## License

Dual-licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
