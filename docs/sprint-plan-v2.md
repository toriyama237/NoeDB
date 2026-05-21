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
| 10 | 🟡 | `SsiChecker` + `record_read` |
| 11 | 🟡 | `DeadlockGuard` |
| 12 | 🟡 | `SchemaCatalog` |
| 13 | 🟡 | GC memtable + LSM active |
| 14 | 🟡 | Bench `oltp_mvcc`, tag `v1.1.0-mvcc` |

Commit : `feat(mvcc): Phase 2 MVCC & transactions` (`0852634`).

## Phase 3 — Performance extrême (S15–S24) 🟡

| Semaine | Statut | Livrable |
|---------|--------|----------|
| 15 | ⬜ | io_uring WAL / SST |
| 16 | ⬜ | mmap zero-copy reads |
| 17 | ⬜ | SIMD predicates |
| 18 | ✅ | Rayon parallel `SeqScan` (`noedb-planner::parallel`) |
| 19 | ⬜ | LZ4 + dict + RLE SST blocks |
| 20 | ⬜ | XOR filter (remplace Bloom) |
| 21 | ⬜ | Query cache LRU |
| 22 | 🟡 | Group commit (`LsmConfig::throughput`, `WalSyncMode::OnFlush`) |
| 23 | ⬜ | Adaptive optimizer |
| 24 | ⬜ | YCSB suite + tag `v1.2.0-perf` |

### Semaine 18 — ✅

- **`parallel::load_table_rows`** : grouping Rayon au-delà de 512 cellules
- Tests : résultats identiques au scan séquentiel

### Semaine 22 — 🟡 (existant)

- **`LsmConfig::throughput()`** : memtable 16 MiB, WAL `OnFlush`, compaction rare

## Phases suivantes

| Phase | Semaines | Thème |
|-------|----------|-------|
| 4 | 25–36 | Distributed elite |
| 5 | 37–44 | Query engine v2 |
| 6 | 45–48 | Observabilité & fiabilité |
| 7 | 49–52 | Ecosystem & **v2.0.0** |
