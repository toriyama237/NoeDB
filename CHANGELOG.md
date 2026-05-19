# Changelog

All notable changes to NoeDB are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
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
- Starter Criterion benchmark for the lexer (`benches/lexer.rs`).
- `rust-toolchain.toml`, `.editorconfig`, `deny.toml`, `cliff.toml` for
  reproducible local builds and changelog generation.

## [0.0.1] - 2026-05-19

### Added
- Day-1 bootstrap of the NoeDB crate.
- Minimal `lexer` module: tokenizes `SELECT <integer>` and rejects anything
  else with `LexError::UnexpectedChar { ch, offset }`.
- 6 unit tests + 1 integration test.
- MIT / Apache-2.0 dual license.
