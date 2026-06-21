## Summary
<!-- What does this MR change and why? -->

## Type
- [ ] feat / fix / perf / refactor
- [ ] storage / raft / engine / CI

## Checklist
- [ ] CI green on LXC 115 (GitLab Runner — no local `cargo build` on ThinkPad)
- [ ] New behaviour covered by tests
- [ ] No secrets committed (`secu-mdp.txt`, tokens, passwords)

## How to verify on runner
```bash
# After merge to main only — deploy stage installs /usr/local/bin/noedb
cargo test -p noedb-storage --release
```
