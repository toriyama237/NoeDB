# Changelog

All notable changes to NoeDB are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- **Clients**: load dev TLS from `<data-dir>/tls/` (`ca.pem`, `client.pem`, `client-key.pem`)
  to match `noedb-cli` layout.

### Added
- **CI**: mdBook build + GitHub Pages deploy (`.github/workflows/docs.yml`).
- **Docs**: [`docs/post-v2-roadmap.md`](docs/post-v2-roadmap.md) for post-sprint follow-ups.

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
