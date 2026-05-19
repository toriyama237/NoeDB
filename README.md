# NoeDB

[![CI](https://github.com/rykielkameni/NoeDB/actions/workflows/ci.yml/badge.svg)](https://github.com/rykielkameni/NoeDB/actions/workflows/ci.yml)
[![Rust stable](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](#license)
[![crates.io](https://img.shields.io/badge/crates.io-not%20published%20yet-lightgrey.svg)](#)

> **A distributed embedded SQL query engine in Rust - think SQLite meets CockroachDB.**

NoeDB is a from-scratch SQL database built in Rust over 52 weeks, brick by brick.
No `sqlx`, no `sled`, no `tokio-postgres` - just the standard library and a few
deliberate dependencies introduced when the design forces them.

---

## Status

**Day 1 / 260** - Week 01 of Phase 1 (Lexer & Parser). The only thing that works today is:

```rust
use noedb::lexer::{tokenize, Token};

let toks = tokenize("SELECT 1")?;
assert_eq!(toks, vec![Token::Select, Token::Number(1), Token::Eof]);
```

That's it. On purpose. This README will grow with the project.

---

## Architecture (target - end of sprint)

```text
   +----------+   +----------+   +--------------+   +------------+   +-----------+
   |  SQL     |   |  Lexer   |   |   Parser     |   |   Query    |   |  Raft     |
   |  text    |-->| (tokens) |-->|    (AST)     |-->|  Planner   |-->| consensus |
   +----------+   +----------+   +--------------+   +------------+   +-----+-----+
                                                                           |
                                                            +--------------v--------------+
                                                            |   LSM Storage Engine        |
                                                            |  (MemTable -> WAL -> SST)   |
                                                            +-----------------------------+
```

---

## Roadmap - 52 weeks

| Phase | Weeks   | Theme                | Milestone tag         |
|-------|---------|----------------------|-----------------------|
| 1     | 01-08   | Lexer & Parser       | `v0.1.0-lexer-parser` |
| 2     | 09-16   | Storage Engine (LSM) | `v0.2.0-storage`      |
| 3     | 17-28   | Query Planner        | `v0.3.0-query-engine` |
| 4     | 29-44   | Raft Consensus       | `v0.4.0-raft`         |
| 5     | 45-52   | Integration & launch | `v1.0.0`              |

**Start:** 2026-05-20 - **Launch Day:** 2027-05-14.

---

## Build & test

```bash
# Install Rust stable (one-time)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# Clone and test
git clone https://github.com/rykielkameni/NoeDB.git
cd NoeDB
cargo test
```

Today, `cargo test` runs ~7 tests. On launch day, it will run thousands.

---

## Why I built this

*(to be written at the end of the sprint - see week 50 in `docs/sprint-plan.md`)*

---

## What was technically hard

*(to be written at the end of the sprint - see week 50 in `docs/sprint-plan.md`)*

---

## Papers & references

Reading list - checked off as each one is applied in code.

- [ ] Ongaro & Ousterhout (2014), *In Search of an Understandable Consensus Algorithm (Raft)*
- [ ] O'Neil et al. (1996), *The Log-Structured Merge-Tree (LSM-Tree)*
- [ ] Graefe (1994), *Volcano - An Extensible and Parallel Query Evaluation System*
- [ ] Bloom (1970), *Space/Time Trade-offs in Hash Coding with Allowable Errors*

---

## License

Dual-licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
