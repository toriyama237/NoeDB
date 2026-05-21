<div align="center">

# NoeDB

[![CI](https://github.com/toriyama237/NoeDB/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/toriyama237/NoeDB/actions/workflows/ci.yml)
[![Security audit](https://github.com/toriyama237/NoeDB/actions/workflows/audit.yml/badge.svg?branch=main)](https://github.com/toriyama237/NoeDB/actions/workflows/audit.yml)
[![CodeQL](https://github.com/toriyama237/NoeDB/actions/workflows/codeql.yml/badge.svg?branch=main)](https://github.com/toriyama237/NoeDB/actions/workflows/codeql.yml)
[![Rust stable](https://img.shields.io/badge/rust-stable-orange.svg?logo=rust)](https://www.rust-lang.org)
[![MSRV 1.78](https://img.shields.io/badge/MSRV-1.78-blue.svg?logo=rust)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![crates.io](https://img.shields.io/badge/crates.io-not%20published%20yet-lightgrey.svg)](#)
[![GitHub stars](https://img.shields.io/github/stars/toriyama237/NoeDB?style=social)](https://github.com/toriyama237/NoeDB/stargazers)

**A distributed embedded SQL query engine in Rust — think SQLite meets CockroachDB.**

Built from scratch over 52 weeks, brick by brick, in public.

</div>

---

## Architecture (target — end of sprint)

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

Each box above is its own Cargo crate inside this workspace. From day 2,
the dependency graph is honest: the lexer cannot depend on the planner,
the planner cannot depend on Raft, etc. This is the same shape Apache
DataFusion uses.

```text
crates/
├── noedb/            # meta-crate, re-exports the user-facing API
├── noedb-lexer/      # tokens, spans, source map  (real code)
├── noedb-ast/        # AST node types             (Phase 1 ✅)
├── noedb-parser/     # recursive-descent parser   (Phase 1 ✅)
├── noedb-planner/    # logical + physical plans   (stub, grows in W17)
├── noedb-storage/    # LSM-tree engine            (stub, grows in W09)
└── noedb-raft/       # consensus                  (stub, grows in W29)
```

- **No `sqlx`, no `sled`, no `tokio-postgres`** — just the standard library and a
  few deliberate dependencies introduced when the design forces them.
- **Every commit is a step in a sprint.** The full plan is in
  [`docs/sprint-plan.md`](docs/sprint-plan.md).
- **CI is non-negotiable.** A red pipeline blocks merge. Always.

---

## Why I built this

I'd been reading database papers for years (Raft, LSM-Tree, Volcano) and using
SQL every day at work — but I still couldn't have explained, end to end, what
happens between `SELECT * FROM users WHERE id = 42;` and the bytes coming back
off disk. NoeDB is the answer to that gap: a database I can actually justify,
line by line, because I wrote every line.

Second, I wanted a 12-month project that forced me to think like a systems
engineer rather than an application developer: a project where correctness,
durability, concurrency, and performance all have to be true at the same time.
A from-scratch distributed SQL engine is exactly that, and it doesn't let you
hide behind a framework.

---

## What was technically hard

> *Filled in as each phase ships. The honest version, not the polished one.*

- **Lexer & Parser (Phase 1)** — shipped Week 08. Pratt precedence for
  `WHERE`, zero-copy identifiers in the lexer, `Display` round-trip on the
  AST. Hardest surprise: disambiguating bare table aliases from column names
  without a full symbol table.
- **LSM Storage Engine (Phase 2)** — *coming Week 16.* I expect the hardest
  parts to be the WAL replay semantics on crash and choosing a sensible compaction
  policy without copying RocksDB's.
- **Query Planner (Phase 3)** — *coming Week 28.* The cost model will lie to me
  at least once.
- **Raft Consensus (Phase 4)** — *coming Week 44.* If history is any guide,
  this is where my unit tests stop being enough.
- **Integration & launch (Phase 5)** — *coming Week 52.*

---

## Status

> **Week 08 / Phase 1 complete** — Lexer, AST, and Parser shipped as `v0.1.0`.

Phase 1 parses `SELECT` (with `JOIN` / `WHERE`), DML (`INSERT`, `UPDATE`,
`DELETE`), and core DDL (`CREATE TABLE`, `DROP TABLE`, `CREATE INDEX`):

```rust
use noedb::parser::parse;
use noedb::ast::Statement;

let stmt = parse("SELECT u.name FROM users u INNER JOIN orders o ON u.id = o.user_id")?;
assert!(matches!(stmt, Statement::Select(_)));
# Ok::<_, noedb::parser::ParseError>(())
```

The lexer tokenizes ~95 % of SQL surface syntax. **200+ tests**, a
**1M-token Criterion bench**, and a **`cargo-fuzz`** target ship with
Phase 1. See [`crates/noedb-lexer/README.md`](crates/noedb-lexer/README.md)
for lexer details.

```text
   SQL text
      │
      ▼
  ┌────────┐     ┌─────────┐     ┌──────────────┐
  │ Lexer  │ ──▶ │ Parser  │ ──▶ │     AST      │
  │ tokens │     │ recursive│     │ Select/DML/  │
  └────────┘     │ + Pratt  │     │ DDL nodes    │
                 └─────────┘     └──────────────┘
```

---

## Roadmap — 52 weeks

| Phase | Weeks   | Theme                | Milestone tag          | Status        |
|-------|---------|----------------------|------------------------|---------------|
| 1     | 01 – 08 | Lexer & Parser       | `v0.1.0-lexer-parser`  | ✅ shipped     |
| 2     | 09 – 16 | Storage Engine (LSM) | `v0.2.0-storage`       | ⏳ planned     |
| 3     | 17 – 28 | Query Planner        | `v0.3.0-query-engine`  | ⏳ planned     |
| 4     | 29 – 44 | Raft Consensus       | `v0.4.0-raft`          | ⏳ planned     |
| 5     | 45 – 52 | Integration & launch | `v1.0.0`               | ⏳ planned     |

**Start:** 2026-05-20 · **Launch day:** 2027-05-14.

---

## Getting started

```bash
# 1. Install Rust stable (one-time)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# 2. Clone and test
git clone https://github.com/toriyama237/NoeDB.git
cd NoeDB
cargo test --all-features
```

Three commands. If it takes more, that's a bug — please open an issue.

### Local development loop

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test  --all-features
cargo doc   --no-deps --all-features
cargo bench --no-run --all-features
```

The exact same checks run in [CI](.github/workflows/ci.yml).

---

## Benchmarks

Micro-benchmarks live in [`crates/noedb-lexer/benches/`](crates/noedb-lexer/benches/)
and are powered by [Criterion](https://bheisler.github.io/criterion.rs/book/).
Run them with:

```bash
cargo bench -p noedb-lexer --bench lexer
open target/criterion/report/index.html
```

The `one_million_tokens` bench targets ~1M tokens per run (debug builds are
slower; use `--release` for representative numbers). Comparative benchmarks
against SQLite land in Phase 2.

---

## Papers & references

Reading list — each entry will be checked off when the corresponding code lands.

- [ ] Ongaro & Ousterhout (2014), *In Search of an Understandable Consensus Algorithm (Raft)*
- [ ] O'Neil et al. (1996), *The Log-Structured Merge-Tree (LSM-Tree)*
- [ ] Graefe (1994), *Volcano — An Extensible and Parallel Query Evaluation System*
- [ ] Bloom (1970), *Space/Time Trade-offs in Hash Coding with Allowable Errors*
- [ ] Selinger et al. (1979), *Access Path Selection in a Relational Database Management System*

---

## Contributing

NoeDB is built in the open. Issues, ideas, and PRs are welcome — see
[CONTRIBUTING.md](./CONTRIBUTING.md) for the full guide. Look for
[`good first issue`](https://github.com/toriyama237/NoeDB/labels/good%20first%20issue)
to find a starter task.

Security reports: please use
[private vulnerability reporting](https://github.com/toriyama237/NoeDB/security/advisories/new).
See [SECURITY.md](./SECURITY.md) for the full policy.

---

## License

Dual-licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual-licensed as above, without any additional terms or conditions.
