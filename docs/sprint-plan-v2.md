# NoeDB v2.0 — Sprint plan (52 semaines)

> **Début :** 2026-06-01 · **Fin cible :** 2027-05-29 · **Base :** v1.0.1

PDF / DOCX détaillé : [`NoeDB_v2_Sprint_Plan.docx`](NoeDB_v2_Sprint_Plan.docx)

## Phase 1 — Security & Protocol (S1–S6) ✅

| Semaine | Statut | Livrable |
|---------|--------|----------|
| 01 | ✅ | TLS 1.3 actif (`noedb-tls`, serveur `--server`) |
| 02 | ✅ | mTLS + SPIFFE CN + pin + reload + `TlsTcpTransport` |
| 03 | ✅ | gRPC (tonic) + proto3, mTLS, `--legacy-tcp` |
| 04 | ✅ | Prepared statements (`PREPARE` / `EXECUTE`, bind typé) |
| 05 | ✅ | Row Level Security (`ENABLE RLS`, `CREATE POLICY`, `SET ROLE`) |
| 06 | ✅ | Audit log JSON, fuzz parser, `SECURITY.md`, `cargo audit` |

### Semaine 1 — ✅

- Crate **`noedb-tls`** : certs dev `rcgen`, `rustls` 1.3, tests handshake
- **`noedb-cli --server`** : TLS (`--no-mtls`) ou mTLS par défaut
- **`noedb-cli --ping`** : client TLS/mTLS + `Ping`

### Semaine 2 — ✅

- **mTLS** : client cert SPIFFE `spiffe://noedb/cluster/node/{id}`
- **Pinning** : `--pin-server` (trust leaf only)
- **Reload** : `ReloadingAcceptor` pour rotation certs
- **Raft** : `TlsTcpTransport` (mTLS + pin)

### Semaine 3 — ✅

- **`noedb-grpc`** : proto3 `Sql` (Execute / Explain / Ping), tonic + vendored `protoc`
- **Sécu** : mTLS 1.3, auth cluster `x-noedb-auth`, cap 1 MiB
- **CLI** : gRPC par défaut (`127.0.0.1:5434`), `--legacy-tcp` pour bincode `:5433`

### Semaine 4 — ✅

- **`PREPARE name AS SELECT … $1`** : cache `PrepareCache`, params `$n`
- **`EXECUTE name (lit, …)`** : bind typé, jamais interpolé
- **Sécu** : injection SQL structurellement impossible sur le chemin prepared

### Semaine 5 — ✅

- **`ALTER TABLE t ENABLE ROW LEVEL SECURITY`**
- **`CREATE POLICY p ON t USING (owner = CURRENT_USER)`**
- **`SET ROLE 'user'`** : filtre injecté dans `WHERE` au planificateur
- **Tests** : user A ne voit jamais les rows de user B

### Semaine 6 — ✅

- **`AuditLog`** : append-only `data/audit/audit.log` (JSON lines)
- **Fuzz** : `cargo fuzz run sql_parser`
- **`SECURITY.md`** : TLS/mTLS/gRPC/RLS/prepared/audit + CVE policy
- **`cargo audit`** : à lancer avant chaque release

## Phase 2 — MVCC & Transactions (S7–S14) ✅

| Semaine | Statut | Livrable |
|---------|--------|----------|
| 07 | ✅ | `Version`, `TimestampOracle`, `MvccMemTable`, LSM `put_version` |
| 08 | ✅ | `TxnManager`, `BEGIN`/`COMMIT`/`ROLLBACK`, read-your-writes |
| 09 | ✅ | `ReadView`, `SnapshotStore`, 100 txns concurrentes |
| 10 | ✅ | `SsiChecker` + `record_read` + `track_rw_dependencies` |
| 11 | ✅ | `DeadlockGuard` (poll on commit) |
| 12 | ✅ | `SchemaCatalog` + `CREATE TABLE` |
| 13 | ✅ | GC memtable + LSM active (`gc` / `gc_mvcc_active`) |
| 14 | ✅ | Bench `oltp_mvcc` Rayon + interior mutability (`eaef836`) |

Commit : `feat(mvcc): Phase 2 MVCC & transactions` (`0852634`).

## Phase 3 — Performance extrême (S15–S24) 🟡

| Semaine | Statut | Livrable |
|---------|--------|----------|
| 15 | ⬜ | io_uring WAL / SST |
| 16 | ⬜ | mmap zero-copy reads |
| 17 | ⬜ | SIMD predicates |
| 18 | ✅ | Rayon parallel `SeqScan` (`noedb-planner::parallel`) |
| 19 | ✅ | LZ4 SST blocks (`compress`, `SST_FLAG_LZ4_BLOCKS`) |
| 20 | ✅ | XOR filter SST v2 (`XorFilter`, `SST_VERSION_V2`) |
| 21 | ✅ | LRU query cache (`noedb-engine::QueryCache`) |
| 22 | ✅ | Group commit (`wal_batch_size`, `maybe_sync_wal`) |
| 23 | ✅ | Adaptive optimizer (`ExecutionFeedback`, `PlanStats`) |
| 24 | ✅ | YCSB + `oltp_mvcc` bench, tag `v1.2.0-perf` |

### Semaine 18 — ✅

- **`parallel::load_table_rows`** : grouping Rayon au-delà de 512 cellules
- Tests : résultats identiques au scan séquentiel

### Semaine 19 — ✅

- **`lz4_flex`** : compression blocs SST (`maybe_compress_block` / `decompress_block`)
- Flag **`SST_FLAG_LZ4_BLOCKS`** sur SST v2

### Semaine 20 — ✅

- **`XorFilter`** : remplace Bloom sur SST v2 (v1 Bloom inchangé)
- Footer `filter_offset` + flags `SST_FLAG_XOR_FILTER`

### Semaine 21 — ✅

- **`QueryCache`** : LRU 4096 entrées, invalidation sur DML / `COMMIT`

### Semaine 22 — ✅

- **`LsmConfig::throughput()`** : memtable 16 MiB, WAL `OnFlush`, `wal_batch_size` 256
- **`maybe_sync_wal`** sur `put` / `put_version`

### Semaine 23 — ✅

- **`ExecutionFeedback`** + **`record_execution`** pour costing adaptatif

### Semaine 24 — ✅

- Bench **`ycsb`** (workloads A/B/C/F) et **`oltp_mvcc`** (transferts avec vérif solde)
- Tag cible **`v1.2.0-perf`**

## Phase 4 — Distributed elite (S25–S36) ✅

| Semaine | Statut | Livrable |
|---------|--------|----------|
| 25 | ✅ | `DistributedEngine` : `Arc`, `RwLock<LsmTree>`, `execute`/`tick` sur `&self` |
| 26 | ✅ | ReadIndex + `execute_select_linearizable` (barrière commit avant SELECT) |
| 27 | ✅ | Joint consensus (`JointConfig`, `propose_conf_change`, `add_voter`) |
| 28 | ✅ | Log compaction + `Action::Snapshot` / `InstallSnapshot` |
| 29–32 | ✅ | `ShardRouter` (hash `(table, row)` → shard) |
| 33–36 | ✅ | `RegionId`, chaos sim (`remove_node`, réélection), tests phase4 |

### Semaine 25 — ✅

- **`DistributedEngine::new_voters` → `Arc<Self>`** : cluster dans `Mutex<Cluster>`
- **Stores** : `Arc<RwLock<LsmTree>>` par replica — SELECT leader en read lock
- **CLI `--cluster`** : backend `Arc<DistributedEngine>` sans mutex global

### Semaine 26 — ✅

- **`RaftCore::read_index`** + RPC `ReadIndex` / `ReadIndexResp`
- **`Cluster::linearizable_barrier`** : commit jusqu’au dernier index du log
- **`DistributedEngine::execute_select_linearizable`** : SELECT cluster via barrière puis lecture LSM leader

### Semaine 27 — ✅

- **`noedb-raft::membership`** : `JointConfig`, encode/decode `ConfChange`, `joint_add_voter` / `joint_finalize`
- **Quorum joint** : `has_quorum_for_index` (majorité outgoing **et** incoming)
- **`Cluster::add_voter`** : conf change `AddVoter` + simulation 4e nœud

### Semaine 28 — ✅

- **`Action::Snapshot`** : compaction log + envoi `InstallSnapshot` aux followers en retard
- Tests : log volumineux (`snapshot_triggers_on_large_log`)

### Semaines 29–36 — ✅

- **`ShardRouter`** : routage déterministe par `(table, row)`
- **`RegionId::LOCAL`** : métadonnée région sur `DistributedEngine`
- **Chaos** : `Cluster::remove_node` (drop RPC + inbox), réélection leader, `propose_on_leader` post-fault
- **Tests** : `noedb-raft/tests/phase4.rs`, `noedb-engine/tests/phase4_distributed.rs`
- Tag cible **`v1.3.0-distributed`**

## Phases suivantes

| Phase | Semaines | Thème |
|-------|----------|-------|
| 4 | 25–36 | Distributed elite (suite) |
| 5 | 37–44 | Query engine v2 |
| 6 | 45–48 | Observabilité & fiabilité |
| 7 | 49–52 | Ecosystem & **v2.0.0** |
