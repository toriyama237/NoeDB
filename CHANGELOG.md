# Changelog

All notable changes to NoeDB are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.2.0] - 2026-07-21

Scan-architecture release: bounded range scans through the whole storage
stack, secondary indexes kept consistent under DML, and a parsed-statement
cache. National audit (50 009 employees): **22/22 PASS**, `IN` subquery
2.8 s → 1.7 s, `NOT EXISTS` 3.5 s → 1.8 s, bulk seed 7.5 s → 6.4 s.

### Performance
- **Storage**: `StorageEngine::range(start, end)` — prefix-bounded scans with
  efficient overrides everywhere:
  - `SstReader::scan_range` seeks via the block index and never reads blocks
    outside `[start, end)`;
  - `LsmTree::range` / `range_visible` merge SSTs + memtable in one bounded,
    MVCC-aware pass (scan windows close the internal-key prefix corner case);
  - `MemTable`, `SnapshotStore`, `TxnOverlayStore` delegate to bounded
    B-tree ranges.
- **Planner / DML**: table scans, row-id loads, secondary-index build /
  load / lookup, UNIQUE checks and row deletes all switched from full
  `iter()` + filter to bounded `range()` — a query now touches only its
  table's keyspace instead of the entire database.
- **Engine**: 256-entry LRU parsed-statement cache (SQL text → AST) on the
  execute path; repeated statements skip lexing/parsing entirely.
  `LocalEngine::statement_cache_stats()` exposes hit/miss counters.

### Fixed
- **Secondary indexes stay consistent under DML** — `CREATE INDEX` used to be
  a one-shot build, so later `INSERT` / `UPDATE` / `DELETE` silently diverged
  and `IndexScan` returned stale or incomplete results:
  - `INSERT` adds MVCC index entries for every indexed column;
  - `UPDATE` tombstones the old-value entry and writes the new one;
  - `DELETE` tombstones the removed row's entries;
  - transactions buffer entry writes and expose them on `COMMIT`.
- `SecondaryIndex::lookup` reads the duplicate-entry keyspace only; the
  unmaintained point-key fast path (which could return rows for values they
  no longer hold) was removed.

### Added
- Integration suites `index_maintenance.rs` (insert / update / delete / txn
  commit through `IndexScan`) and `stmt_cache.rs` (AST-cache hits, freshness
  after `UPDATE`); storage unit tests for `scan_range` and bounded `range`
  equivalence across SSTs + memtable + MVCC.

### Quality
- `cargo clippy --workspace --all-targets`: **0 warnings**; full `rustfmt`.
- Workspace suite: **330 tests, 0 failures**; national audit 22/22 PASS.

## [2.1.0] - 2026-07-21

Enterprise hardening release: planner correctness and performance validated by
a national-scale audit (80 agencies / 50 009 employees / 50 009 payslips —
**22/22 benchmarks PASS**), UTF-8 SQL, and an audit-ready git workflow.

### Performance
- **Planner**: hash semi-join for decorrelated `IN (SELECT …)` and
  `EXISTS` / `NOT EXISTS` — replaces the O(n·m) nested loop
  (national audit: `IN` 221 s → 2.8 s; `NOT EXISTS` 329 s → 3.5 s).
- **Planner**: `ORDER BY … LIMIT k` fused into a partial top-k sort
  (`select_nth_unstable_by`); `EXPLAIN` shows `Sort(top_k=N)`.

### Fixed
- **Planner**: hash-join keys are now oriented to the join tree's left/right
  inputs — `ON e.agency_id = ag.id` (reversed operand order) returned 0 rows.
- **Planner**: CTE column resolution — outer `SELECT`/`ORDER BY` referencing a
  bare column (`country`) stored qualified in the CTE (`ag.country`) raised
  `UnknownColumn`; shared suffix-aware matching in pruning and sort keys.
- **INSERT atomicity**: all cells validated and encoded before any LSM write —
  a failing `NOT NULL` no longer leaves ghost partial rows.
- **HAVING**: aggregate matching by value (not span) — `HAVING COUNT(*) > n`
  no longer fails with `UnsupportedExpr`.
- **LEFT JOIN**: propagated `left_outer` through logical → physical plans;
  previously executed as INNER.
- **MVCC GC**: no longer collects the latest committed version.
- **RLS**: row-level security enforced on `UPDATE` / `DELETE` (was SELECT-only).
- **ALTER TABLE**: gated to admin roles.
- **Clients**: load dev TLS from `<data-dir>/tls/` (`ca.pem`, `client.pem`,
  `client-key.pem`) to match `noedb-cli` layout.

### Added
- **Lexer**: UTF-8 accepted in string literals and quoted identifiers
  (`'Direction Générale'` now lexes; previously `UnexpectedChar`).
- **Example**: `national_hr_audit` — end-to-end enterprise audit binary
  (bulk load, 20 business SQL benchmarks, constraint checks, JSON report).
- **Docs**: [`docs/git-workflow.md`](docs/git-workflow.md) — audit-ready
  branching model (feature branches, Conventional Commits, `--no-ff` merges).
- **NoeDB Studio**: `./scripts/noedb-studio.sh` — branded bash launcher + `--studio` REPL
  (ASCII banner, boot animation, table output, `noedb›` prompt).
- **YCSB smoke**: `post_v2_ycsb.rs` (workloads A/B/C/F at miniature scale).
- **Grafana**: starter dashboard `docs/grafana/noedb-overview.json`.
- **CI**: mdBook build + GitHub Pages deploy (`.github/workflows/docs.yml`).
- **Docs**: [`docs/post-v2-roadmap.md`](docs/post-v2-roadmap.md) for post-sprint follow-ups.

### Quality
- `cargo clippy --workspace --all-targets` emits **0 warnings** (pedantic on,
  `-D warnings` in CI); full `rustfmt` pass.
- Workspace suite: **316 tests, 0 failures**.

## [2.0.0] - 2026-05-20

### Added
- **Language clients** (gRPC + dev TLS): Python (`clients/python`), Go (`clients/go`),
  Node.js (`clients/nodejs`).
- **`noedb-pool`**: gRPC connection pool, periodic health checks, read/write load balancer.
- **mdBook documentation** under `book/` (architecture, storage, Raft, query engine,
  banking tutorial, clients, observability).
- **Phase 6** (in this release line): `noedb-metrics`, Prometheus `/metrics`, fuzz,
  Raft chaos/proptest tests.

### Changed
- Workspace version **2.0.0** (from 1.0.1 / 1.4.0-query feature line).

## [1.4.0-query] - 2026-05-20

### Added
- **Phase 5 query engine v2** complete: window functions, `IN (SELECT …)` decorrelation,
  CTEs (`WITH RECURSIVE`), set ops (`UNION` / `INTERSECT` / `EXCEPT`), `CAST`, column
  statistics (`ANALYZE TABLE`), and cost model v2 with `EXPLAIN rows≈N`.
- **TPC-H lite** benchmark: `cargo bench -p noedb-engine --bench tpch_lite` (7 analytical
  queries on a miniature star schema).

## [1.0.1] - 2026-05-20

### Performance
- **Engine:** cache Raft leader; no `tick(80)` on every `SELECT` (~42% faster distributed SELECT bench).
- **Raft sim:** `drive_quiescent` replaces heavy `run_rounds(40)` per propose; bench warmup 80 rounds not 400.
- **Lexer benches:** all use `tokenize_into` (allocator-free); 1M-token path unchanged (~16–34 ms machine-dependent).

### Fixed
- **CLI:** clearer REPL banner; ignore `#` / `cargo` lines pasted by mistake.

## [1.0.0] - 2026-05-20

### Added
- **Phase 5 integration (Weeks 45–52):** [`noedb-engine`] wires parser → planner →
  Raft → LSM with bounded SQL input, [`LocalEngine`] and [`DistributedEngine`]
  (3-node in-process cluster).
- [`noedb-protocol`]: authenticated bincode frames (`ClusterAuth`, 1 MiB cap).
- **`noedb` CLI** (`cargo run -p noedb-cli`): interactive REPL, `--cluster` mode,
  TCP server (`--server --listen 127.0.0.1:5433`).
- 20-query E2E corpus and `cargo bench -p noedb-engine --bench pipeline`.

### Security
- `MAX_SQL_BYTES` (64 KiB) and NUL rejection on all engine entry points.
- `MAX_COMMAND_BYTES` (64 KiB) on replicated log payloads.
- Wire protocol reuses Raft envelope verification.

[`noedb-engine`]: crates/noedb-engine/src/lib.rs
[`LocalEngine`]: crates/noedb-engine/src/engine.rs
[`DistributedEngine`]: crates/noedb-engine/src/engine.rs
[`noedb-protocol`]: crates/noedb-protocol/src/lib.rs

## [0.4.0] - 2026-05-20

### Added
- **Phase 4 Raft (Weeks 31–37, 44):** [`noedb-raft`] with pure core FSM
  (`Raft`), election (`RequestVote`), log replication (`AppendEntries`),
  `InstallSnapshot` / `ReadIndex` RPCs, `RaftStorage` (`MemStorage`,
  `FileStorage` with CRC-32), `RaftNode` runtime, bincode wire codec with
  [`ClusterAuth`] + bounded frames, in-process [`Cluster`] simulator, and
  Tokio length-prefixed TCP transport.
- Criterion bench `cargo bench -p noedb-raft --bench raft` (~37k commands/s
  on 3-node in-process cluster for 1k proposals).

### Not in this release (planned W38–43)
- Joint-consensus membership changes, `madsim` fuzz, TLS, full linearizable
  read integration, production failover tests.

### Fixed
- Lexer 1M-token benchmark regression (~30 ms → ~16 ms): [`tokenize_into`],
  whitespace skip, single-digit integer fast path, `[profile.bench] debug = false`.
- Raft replication ping-pong: leader only re-sends `AppendEntries` when
  `next_index <= last_index` (heartbeats on tick only).

[`noedb-raft`]: crates/noedb-raft/src/lib.rs
[`ClusterAuth`]: crates/noedb-raft/src/security.rs
[`Cluster`]: crates/noedb-raft/src/sim.rs
[`tokenize_into`]: crates/noedb-lexer/src/lib.rs

## [0.3.0] - 2026-05-20

### Added
- **Phase 3 query engine (Weeks 17–28):** [`noedb-planner`] with logical plans
  (`Scan`, `Filter`, `Project`, `Join`, `Aggregate`, `Sort`, `Limit`),
  physical operators (`SeqScan`, `IndexScan`, `HashJoin`, `NestedLoopJoin`,
  `MergeJoin`, hash `Aggregate`, `Sort`, `Limit`), and Volcano executors over
  [`LsmTree`].
- **B-tree secondary indexes** persisted in the LSM (`SecondaryIndex`, point
  lookup via `LsmTree::get`).
- **Cost model v1** and cost-based optimizer: predicate pushdown, projection /
  column pruning, join algorithm pick, index selection (`EXPLAIN` output).
- **`explain_sql`**, **`apply_statement`** (`CREATE INDEX`), Criterion bench
  `cargo bench -p noedb-planner --bench query` (SeqScan vs IndexScan on 100k rows).

### Changed
- Workspace version bumped to `0.3.0`.
- `execute_sql` runs the optimizer (was naive `lower()` only).

[`noedb-planner`]: crates/noedb-planner/src/lib.rs
[`LsmTree`]: crates/noedb-storage/src/lsm.rs

## [0.2.0] - 2026-05-20

### Added
- **Week 16 LSM engine:** [`LsmTree`] orchestrates WAL segments, MemTable flush,
  L0/L1 SSTables, and L0→L1 compaction; Criterion bench (`cargo bench -p noedb-storage --bench lsm`).
- **Week 15 compaction:** `compact_level0_to_l1` k-way merge of L0 SSTables into L1.
- **Weeks 12–14 SSTable stack:** `SstWriter` / `SstReader` with 4 KiB blocks, sparse
  block index, footer metadata, and embedded [`BloomFilter`] (double hashing + CRC32).
- **Week 11 WAL segments:** [`WalSegmentManager`] with numbered `.wal` files, rotation,
  replay on startup, and segment deletion after flush.
- **Week 09–10 storage:** CRC-32 checksums, append-only [`Wal`] with `sync_all`,
  [`LogEntry`] (`Put`/`Delete` + CRC32), [`DurableStore`] (WAL-first writes,
  `max_mem_bytes` MemTable rotation with immutable swap).
- **Week 09 storage:** `StorageEngine` trait, `MemTable` backed by `BTreeMap`,
  `get`/`put`/`delete`, sorted iterator, range scan, 1 000-entry integration test.

### Fixed
- SSTable header padding (`HEADER_LEN = 16`) so on-disk offsets match the writer's
  tracked file position (fixes index/bloom read corruption).

[`LsmTree`]: crates/noedb-storage/src/lsm.rs
[`WalSegmentManager`]: crates/noedb-storage/src/wal/segments.rs
[`BloomFilter`]: crates/noedb-storage/src/bloom.rs

## [0.1.0] - 2026-05-20

### Added
- **Phase 1 parser (Weeks 05–08):** recursive-descent parser with Pratt
  expression precedence. Parses `SELECT` (projections, `FROM`, `INNER`/`LEFT JOIN`,
  `WHERE` with `AND`/`OR`/`NOT`, `IS NULL`, `IN`, `BETWEEN`, `LIKE`),
  DML (`INSERT` multi-row, `UPDATE`, `DELETE`), and DDL (`CREATE TABLE`,
  `DROP TABLE`, `CREATE INDEX`).
- **`noedb-ast`:** `SelectStmt`, `InsertStmt`, `UpdateStmt`, `DeleteStmt`,
  `CreateTableStmt`, `DropTableStmt`, `CreateIndexStmt`, expression types,
  and `Display` for SQL round-trip.
- **Week 04 lexer hardening:** `LexError::line_column()`, 200+ corpus tests,
  1M-token Criterion benchmark, `cargo-fuzz` target.
- **`INDEX` reserved keyword** for `CREATE INDEX`.
- 17 parser integration tests including round-trip `parse → Display → parse`.

### Changed
- Workspace version bumped to `0.1.0`.
- `noedb::parser::parse()` is fully implemented (was `NotYetImplemented`).
- Meta-crate smoke tests exercise the live parser API.

## [0.0.1] - 2026-05-19

### Added
- Day-1 bootstrap of the NoeDB crate.
- Minimal `lexer` module: tokenizes `SELECT <integer>` and rejects anything
  else with `LexError::UnexpectedChar { ch, offset }`.
- 6 unit tests + 1 integration test.
- MIT / Apache-2.0 dual license.
