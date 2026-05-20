# noedb-lexer

> The lexer for [NoeDB]. Turns SQL text into a stream of tokens with
> byte-precise spans.

This crate is part of the [NoeDB] workspace. It is intentionally
zero-dependency (no `serde`, no `thiserror`, no `regex`) so that
building the lexer never blocks on transitive churn.

## Design notes

- **Spans, not just offsets.** Every token carries a `Span { start, end }`
  with byte positions into the original input. Line/column are derived
  on demand by [`SourceMap`], not stored eagerly. UTF-8 safe.
- **Forbid `unsafe`.** Enforced at the crate level by
  `#![forbid(unsafe_code)]`.
- **Doctests on every public item.** If it's in the public API, it has
  an `# Examples` block that runs in CI.

## Status

> **Day 2 / 260** — Week 01 of Phase 1. Today the lexer only knows the
> `SELECT` keyword and base-10 integer literals. The Week 04 milestone
> is a 40+ variant Token enum covering SQL-92.

See the full plan in [`docs/sprint-plan.md`](../../docs/sprint-plan.md).

[NoeDB]: https://github.com/toriyama237/NoeDB
[`SourceMap`]: https://docs.rs/noedb-lexer/latest/noedb_lexer/struct.SourceMap.html
