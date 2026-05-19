<!--
Thanks for contributing to NoeDB!
The PR title should be a single Conventional Commit summary line, e.g.
  feat(lexer): support BETWEEN keyword
  fix(parser): correct precedence of unary minus
-->

## What

<!-- One or two sentences describing the change. -->

## Why

<!-- The problem this PR solves, or the use case it unlocks. -->

## How

<!-- Implementation notes worth flagging to a reviewer (trade-offs, alternatives). -->

## How to verify

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test  --all-features
```

## Checklist

- [ ] PR title is a Conventional Commit summary.
- [ ] Tests added or updated.
- [ ] Public API has rustdoc (with `# Examples` when it makes sense).
- [ ] `CHANGELOG.md` updated under `## [Unreleased]` (skip for `chore:` / `ci:` only).
- [ ] CI is green.

Closes #
