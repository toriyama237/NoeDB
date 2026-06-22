# Contributing to NoeDB

Thanks for your interest in NoeDB! This is a public, 52-week learning-in-the-open
project: every commit is a step in a sprint, and external contributions are very
welcome - especially small ones.

> **TL;DR** — fork, branch off `main`, run `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`, open a PR with a [Conventional Commit](https://www.conventionalcommits.org/en/v1.0.0/) title.

---

## Ground rules

- Be kind. The [Code of Conduct](./CODE_OF_CONDUCT.md) applies everywhere.
- Discuss large changes **before** writing code. Open an issue or a Discussion.
- Small fixes (typos, doc improvements, missing tests) don't need an issue first.
- This is a from-scratch DB: heavy external dependencies need a strong reason.
  The default answer is "no, write it yourself".

## Essingan / OVH dev loop (ThinkPad = code only)

On the Spitzkop Essingan sandbox, **the ThinkPad never compiles**. It only edits
and pushes; **GitLab CI on LXC 115 (AMD EPYC)** runs `cargo build`, tests, and
benchmarks.

```bash
# 1. Tunnel (keep open)
ssh -N -L 8480:192.168.3.249:80 -L 2229:192.168.3.249:2222 root@217.182.95.243

# 2. Feature branch (never commit parano/storage work directly on main)
git checkout -b feat/my-change
# … edit code only — no cargo build required on ThinkPad …
git add -p && git commit -m "feat(storage): …"

# 3. Push feature branch → GitLab opens MR pipeline on LXC 115
./scripts/infra/push-feature.sh

# 4. Open MR in browser → merge to main when CI is green
#    http://127.0.0.1:8480/root/noedb/-/merge_requests
```

After merge to `main`, the `deploy:runner` job installs `/usr/local/bin/noedb`
on LXC 115. GitHub `origin` stays optional; GitLab is the CI source of truth.

## Development setup

You need:

- Rust **stable** (1.88+; `rust-toolchain.toml` pins the channel).
- `git`.
- Optional: `cargo-deny`, `cargo-llvm-cov`, `git-cliff`.

```bash
git clone https://github.com/toriyama237/NoeDB.git
cd NoeDB
# Local build optional — on Essingan sandbox use GitLab CI instead:
# cargo build --all-features
# cargo test  --all-features
```

## Local checks (optional on ThinkPad; mandatory in GitLab CI)

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test  --all-features
cargo doc   --no-deps --all-features
cargo bench --no-run --all-features
```

The exact same checks run in [CI](.github/workflows/ci.yml). Mismatches between
local and CI are bugs - please report them.

## Commit style

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).
The first line drives the changelog (see `cliff.toml`):

| Prefix     | Meaning                                                 |
|------------|---------------------------------------------------------|
| `feat:`    | A new feature                                           |
| `fix:`     | A bug fix                                               |
| `perf:`    | A performance improvement                               |
| `refactor:`| Internal change, no behaviour change                    |
| `doc:`     | Documentation only                                      |
| `test:`    | Tests only                                              |
| `build:`   | Build system / dependencies                             |
| `ci:`      | CI configuration                                        |
| `chore:`   | Anything else (release commits, housekeeping)           |

Add a scope when useful: `feat(lexer): support BETWEEN`.

Breaking changes: add `!` after the type or a `BREAKING CHANGE:` footer.

## Pull request checklist

Before requesting review, please confirm:

- [ ] The PR title is a single Conventional Commit summary line.
- [ ] CI is green (or you have a comment explaining why a check is allowed to fail).
- [ ] New behaviour is covered by tests (unit, integration, or both).
- [ ] Public items have rustdoc with an `# Examples` section when relevant.
- [ ] The change is described in the PR body: *what*, *why*, *how to verify*.

## Good first issues

Issues labelled `good first issue` are scoped to be doable in a few hours by a
newcomer to the codebase. If a label looks stale, comment to claim it and we'll
re-assign it. **One issue per PR**.

## Releasing

Releases are tag-driven. A maintainer:

1. Updates `Cargo.toml`'s `version`.
2. Regenerates `CHANGELOG.md` with `git cliff -o CHANGELOG.md`.
3. Commits with `chore(release): vX.Y.Z`.
4. Tags `vX.Y.Z` and pushes the tag.

The [`Release`](.github/workflows/release.yml) workflow takes over from there.

## Questions?

Open a thread in [GitHub Discussions](https://github.com/toriyama237/NoeDB/discussions).
