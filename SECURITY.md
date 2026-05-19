# Security Policy

## Supported versions

NoeDB is pre-1.0 and under heavy active development. Only the latest commit on
`main` and the most recent tagged release receive security fixes.

| Version            | Supported          |
|--------------------|--------------------|
| `main` (HEAD)      | :white_check_mark: |
| latest `vX.Y.Z`    | :white_check_mark: |
| anything else      | :x:                |

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Instead, use GitHub's private vulnerability reporting:

1. Go to <https://github.com/toriyama237/NoeDB/security/advisories/new>.
2. Fill in the form. Include a minimal reproduction if possible.
3. We aim to acknowledge within **72 hours** and to ship a fix or a mitigation
   within **30 days** for high-severity issues.

If GitHub's flow is not available to you, email the maintainers via the address
shown on the GitHub profile of [@toriyama237](https://github.com/toriyama237)
with the subject line `SECURITY: NoeDB`.

## Scope

In scope:

- The `noedb` crate (library code under `src/`).
- The build, test, and release workflows under `.github/workflows/`.

Out of scope (please report upstream):

- Vulnerabilities in third-party dependencies (please open an issue at the
  vendor and reference it here).
- Issues that require a fork of NoeDB with malicious local changes.
