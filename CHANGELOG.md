# Changelog

All notable changes to NoeDB are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Day 2 lexer (Week 02 deliverable):** peekable `Cursor`, public `Lexer` iterator API, 80+ SQL-92 keywords via sorted static table (zero-allocation lookup), `Token::Ident` (zero-copy lexeme), `Token::Integer` / `Token::Float`, `Token::String` with `''` escape, `Token::QuotedIdent` with `""` escape.
- **Workspace layout.** 7 crates under `crates/`: `noedb`, `noedb-lexer`, `noedb-ast`, `noedb-parser`, `noedb-planner`, `noedb-storage`, `noedb-raft`. The meta-crate `noedb` re-exports the user-facing API.
- **Byte-precise `Span` everywhere.** `SourceMap` resolves byte offsets to `(line, column)` in `O(log n)`.
- **`SpannedToken { kind, span }`** replaces bare tokens in `tokenize`'s return type.
- 30+ lexer unit/integration tests; 3 Criterion benchmarks.

### Fixed
- `SourceMap::line_column` when the byte offset falls exactly on a `\n`.

### Changed
- `Token::Select` / `Token::Number` replaced by `Token::Keyword(Keyword::…)` and `Token::Integer` (breaking API change on day 2).

### Added (infrastructure, day 1)
- **Workspace layout (Day 2).** The crate is now a Cargo workspace with
  7 members under `crates/`: `noedb`, `noedb-lexer`, `noedb-ast`,
  `noedb-parser`, `noedb-planner`, `noedb-storage`, `noedb-raft`. The
  meta-crate `noedb` re-exports the user-facing API; the others are
  independently publishable.
- **Byte-precise `Span` everywhere.** Every token returned by the lexer
  carries a `Span { start: u32, end: u32 }`. `SourceMap` precomputes a
  newline index once and resolves any byte offset to `(line, column)`
  in `O(log n)`.
- **`SpannedToken { kind, span }`** replaces the bare `Token` enum in
  `tokenize`'s return type. Error variants carry their `Span` too.
- **Workspace inheritance.** Lints, MSRV, repo metadata and shared
  dependencies are defined once at the workspace root and inherited via
  `[lints] workspace = true` and `package.workspace = true`.
- **Sprint plan.** `docs/sprint-plan.md` documents what happens on each
  of the 260 days.
- **Per-crate `README.md`s** for `noedb-lexer` (more will follow).
- **Cross-crate integration tests** in `crates/noedb/tests/smoke.rs`
  exercising the re-export shape.

### Changed
- `cargo test` now runs `--workspace`; the previous single-crate layout
  is gone.
- Old `src/lexer.rs` (4.5 KB) split into `span.rs`, `error.rs`,
  `token.rs`, `cursor.rs`, `lib.rs` under `crates/noedb-lexer/src/`.
- `LexError` now exposes a structured `LexErrorKind`
  (`UnexpectedChar`, `UnknownIdentifier`, `NumberOutOfRange`) rather
  than a single `UnexpectedChar` variant. Numbers that don't fit in
  `i64` are now a real error rather than a generic char error.

### Added (Day 1, kept for posterity)
- Production-grade CI: `fmt`, `clippy -D warnings`, multi-OS test matrix
  (stable + beta on Linux/macOS/Windows), MSRV check (1.78), `cargo doc`,
  benchmark compile, coverage with `cargo-llvm-cov`, and spell-check with
  `typos`.
- Security workflows: `cargo audit`, `cargo deny`, and CodeQL for GitHub
  Actions.
- Tag-driven release workflow that builds Linux/macOS/Windows artifacts and
  publishes a GitHub release with auto-generated notes.
- Weekly Dependabot updates for both Cargo and GitHub Actions.
- Community files: `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
  `CODEOWNERS`, issue and PR templates.
- Starter Criterion benchmark for the lexer.
- `rust-toolchain.toml`, `.editorconfig`, `deny.toml`, `cliff.toml` for
  reproducible local builds and changelog generation.

## [0.0.1] - 2026-05-19

### Added
- Day-1 bootstrap of the NoeDB crate.
- Minimal `lexer` module: tokenizes `SELECT <integer>` and rejects anything
  else with `LexError::UnexpectedChar { ch, offset }`.
- 6 unit tests + 1 integration test.
- MIT / Apache-2.0 dual license.
