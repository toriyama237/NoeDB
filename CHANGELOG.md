# Changelog

All notable changes to NoeDB are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
