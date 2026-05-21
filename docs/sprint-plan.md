# NoeDB - 52-week sprint plan

> **Start:** 2026-05-20 — **Launch day:** 2027-05-14.

This document is the single source of truth for what happens which week.
Every commit should be traceable to a line below.

## Conventions

- Each phase ends with a tagged release (`vX.Y.Z`) and a LinkedIn post.
- "✅" = shipped, "🟡" = in progress, "⏳" = planned.
- Day numbers are 1-indexed (Day 1 = 2026-05-20, the day the repo went
  public). **Day 2 = 2026-05-21.** **Day 3 = 2026-05-22.**

---

## Phase 1 — Lexer & Parser (Weeks 01-08)

| Week | Lundi | Mardi | Mercredi | Jeudi | Vendredi | Livrable |
|------|-------|-------|----------|-------|----------|----------|
| 01 | Setup repo, Cargo.toml, CI ✅ | Workspace + arbo lexer/parser/ast/storage/planner/raft ✅ | Lire spec SQL-92 (SELECT / INSERT / CREATE) | Définir enum Token avec 40+ variantes | Premier test lexer : tokeniser `SELECT 1` ✅ | Repo public, CI verte, Token enum compilé |
| 02 | Lexer: chars peekable iterator ✅ | Méthode `next_token()`: switch sur char courant ✅ | Identifiants + mots-clés (table statique triée) ✅ | Entiers et flottants ✅ | Chaînes `'` + identifiants `"` ✅ | Lexer tokenise identifiants, nombres, chaînes ✅ |
| 03 | Opérateurs simples: `=`, `!=`, `<`, `>`, `<=`, `>=`, `AND`, `OR` ✅ | Opérateurs composés: `IS NULL`, `LIKE`, `IN`, `BETWEEN` (keywords séparés) ✅ | Ponctuation: `(`, `)`, `,`, `;`, `.`, `*` ✅ | Commentaires `--` et `/* */` ✅ | Mots-clés réservés (80+) ✅ | Lexer 95 % des tokens SQL couverts ✅ |
| 04 | Tests unitaires exhaustifs: 200+ cas ✅ | Error recovery: `LexError` position ligne/col ✅ | Benchmark: tokenise 1M tokens en < 50 ms ✅ | Documentation rustdoc sur chaque méthode ✅ | 100 % tests passants, benchmark publié sur README ✅ | Tests, fuzz `cargo-fuzz`, benchmark publié ✅ |
| 05 | Design AST: `SelectStmt`, `InsertStmt`, `CreateTable` ✅ | Types expr: `BinaryExpr`, `UnaryExpr`, `Literal`, `Column` ✅ | Parser struct + méthode `parse()` entrypoint ✅ | `parse_select()` recursive descent SELECT basique ✅ | Test: parser `SELECT a, b FROM t`  → AST correct ✅ | Types AST définis, `parse_select()` opérationnel ✅ |
| 06 | `parse_where()` — expressions booléennes imbriquées ✅ | Gestion précédence opérateurs (Pratt parsing) ✅ | `parse_from()` — tables simples et alias ✅ | `parse_join()` — INNER JOIN, LEFT JOIN ✅ | Tests intégration SELECT complet avec JOIN+WHERE ✅ | SELECT complet avec JOIN, WHERE, alias parsé ✅ |
| 07 | `parse_insert()` — VALUES multiples ✅ | `parse_update()` — SET list, WHERE ✅ | `parse_delete()` — WHERE clause ✅ | `parse_create_table()` — colonnes + types + NOT NULL ✅ | `parse_drop_table()`, `parse_create_index()` ✅ | DDL + DML complets: INSERT/UPDATE/DELETE/CREATE ✅ |
| 08 | Error recovery: skip token jusqu'au prochain `;` ✅ | Tests round-trip: input → Display → re-parse ✅ | 1er article LinkedIn "J'ai écrit un parser SQL" ⏳ | README Phase 1 avec diagramme AST ✅ | Tag git `v0.1.0-lexer-parser`, release notes ✅ | **v0.1.0** publiée + doc Phase 1 ✅ |

**Phase 1 livrables** : `noedb-lexer` + `noedb-ast` + `noedb-parser` matures, ~ 95 % SQL-92 couvert, 200+ tests, 1 article LinkedIn.

---

## Phase 2 — Storage Engine (LSM) (Weeks 09-16)

| Week | Theme | Livrable |
|------|-------|----------|
| 09 | Design Storage API: trait `StorageEngine`, MemTable `BTreeMap<Vec<u8>, Vec<u8>>`, opérations get/put/delete, iterator ✅ | `MemTable` CRUD opérationnel avec test d'itération triée ✅ |
| 10 | WAL append-only + `LogEntry` (OpType, CRC32), `max_mem_bytes` rotation ✅ | WAL fonctionnel, écritures durables avant MemTable ✅ |
| 11 | WAL replay au démarrage → reconstruire MemTable; test crash: process kill → restart → data OK ✅ | WAL recovery testé et validé, 0 perte de données ✅ |
| 12 | SSTable format binaire: magic bytes + version header, SSTable Writer avec blocs triés 4KB, index sparse en fin de fichier ✅ | SSTable Writer ↔ format binaire stable (`NOES` v1) ✅ |
| 13 | SSTable Reader: lire footer + index sparse, recherche binaire, scan bloc, iterator full SSTable ✅ | SSTable Reader opérationnel avec recherche O(log n) ✅ |
| 14 | Bloom filter: double hashing maison, intégration SSTable (sérialisation footer-adjacent) ✅ | Bloom filter from scratch, FPR mesuré < 2 % ✅ |
| 15 | Compaction leveled: merge L0→L1, déclenchement par nombre de fichiers L0 ✅ | Compaction k-way merge fonctionnelle ✅ |
| 16 | `LsmTree` E2E (WAL segments + MemTable + SST + compaction), benchmarks Criterion, tag `v0.2.0-storage` ✅ | **v0.2.0** publiée ✅ |

---

## Phase 3 — Query Planner (Weeks 17-28) ✅

**Tag:** `v0.3.0-query-engine` — shipped 2026-05-20.

| Week | Theme |
|------|-------|
| 17 | Logical plan : `Scan`, `Filter`, `Project`, `Join`, `Aggregate`, `Sort`, `Limit` |
| 18 | AST → LogicalPlan: passes successives |
| 19 | Physical plan : opérateurs Volcano (next-tuple iterator) |
| 20 | `SeqScan`, `Filter`, `Project` physiques sur SSTable |
| 21 | `HashJoin` build/probe |
| 22 | `NestedLoopJoin`, `MergeJoin` |
| 23 | `Aggregate` hash-based + streaming |
| 24 | Cost model v1 : cardinalités + I/O cost |
| 25 | Optimizer: predicate pushdown |
| 26 | Optimizer: projection pushdown + column pruning |
| 27 | Optimizer: index selection (cost-based) |
| 28 | Benchmarks : SeqScan 500 ms vs IndexScan 4 ms (125× speedup), tag `v0.3.0-query-engine`, post LinkedIn #3 |

---

## Phase 4 — Raft Consensus (Weeks 29-44) ✅

**Tag:** `v0.4.0-raft` — shipped 2026-05-20 (core + bench; W38–43 follow in Phase 5).

| Week | Theme | Status |
|------|-------|--------|
| 29-30 | Lire le paper Raft (Ongaro & Ousterhout, 2014) ligne par ligne | ✅ |
| 31 | États : Follower, Candidate, Leader | ✅ |
| 32 | Élection : RequestVote RPC | ✅ |
| 33 | Réplication : AppendEntries RPC | ✅ |
| 34 | Persistence : term, votedFor, log | ✅ |
| 35-36 | Networking : `tokio` + bincode | ✅ |
| 37 | Cluster 3 nœuds : élection + replication OK | ✅ |
| 38 | Tolérance aux pannes : kill leader, nouveau leader élu < 1 s | ⏳ sim only |
| 39 | Membership changes (joint consensus) | ⏳ |
| 40 | Log compaction + snapshots | 🟡 RPC + stub |
| 41-42 | Linearizable reads + leases | 🟡 ReadIndex RPC |
| 43 | Fuzz cluster avec `madsim` | ⏳ |
| 44 | Benchmark Raft : 10k writes/s @ N=3 nœuds, tag `v0.4.0-raft` | ✅ ~37k cmd/s sim |

---

## Phase 5 — Integration & launch (Weeks 45-52)

| Week | Theme |
|------|-------|
| 45 | Brancher Parser → Planner → Storage → Raft de bout en bout |
| 46 | CLI `noedb` interactif |
| 47 | Driver wire-protocol minimal (Postgres-like) |
| 48 | Stress tests : YCSB workload A/B |
| 49 | Documentation: book mdbook |
| 50 | Rédiger "Why I built this" + "What was technically hard" du README |
| 51 | Préparer Show HN + thread Twitter/LinkedIn |
| 52 | **v1.0.0** publiée + post LinkedIn long + Show HN |

---

## Weekly LinkedIn cadence

| Sprint week | Post |
|---|---|
| 08 | "J'ai écrit un parser SQL from scratch" — extrait code + AST |
| 16 | "Ce que j'ai appris en implémentant un Storage Engine" — LSM vs B-Tree, Bloom filter |
| 28 | "Mon Query Optimizer décide entre SeqScan et IndexScan" — cost model, benchmark 125× |
| 42 | "J'ai implémenté Raft from scratch en Rust" — storytelling + bugs |
| 52 | Article long : "Ce que 12 mois à construire NoeDB m'ont appris" + Show HN |
