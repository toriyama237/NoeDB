# Changelog

All notable changes to NoeDB are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
