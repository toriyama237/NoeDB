#!/usr/bin/env bash
# Start a new feature branch for GitLab MR workflow (ThinkPad writes, LXC 115 compiles).
set -euo pipefail

NAME="${1:?usage: new-feature.sh feat-name-slug}"

cd "$(git rev-parse --show-toplevel)"
BASE="${GITLAB_BASE_BRANCH:-main}"

git fetch gitlab "$BASE" 2>/dev/null || true
git checkout "$BASE"
git pull gitlab "$BASE" 2>/dev/null || true
git checkout -b "feat/${NAME}"

echo "Branch feat/${NAME} ready — edit code, commit, then:"
echo "  ./scripts/infra/push-feature.sh"
