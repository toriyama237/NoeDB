#!/usr/bin/env bash
# Push current feature branch to GitLab OVH — ThinkPad writes code, LXC 115 compiles in CI.
set -euo pipefail

BRANCH="${1:-$(git branch --show-current)}"
GITLAB_HTTP="${GITLAB_HTTP:-http://127.0.0.1:8480}"
GITLAB_SSH="${GITLAB_SSH:-ssh://git@127.0.0.1:2229}"
PROJECT="${GITLAB_PROJECT:-root/noedb}"

cd "$(git rev-parse --show-toplevel)"

if [[ "$BRANCH" == "main" ]]; then
  echo "Use push-to-gitlab.sh for main, or pass an explicit feature branch name." >&2
  exit 1
fi

if ! git remote get-url gitlab >/dev/null 2>&1; then
  git remote add gitlab "${GITLAB_HTTP}/${PROJECT}.git"
fi

echo "→ Pushing branch '${BRANCH}' to GitLab (CI builds on LXC 115)…"
git push -u gitlab "${BRANCH}"

echo ""
echo "Open MR:"
echo "  ${GITLAB_HTTP}/${PROJECT}/-/merge_requests/new?merge_request[source_branch]=${BRANCH}"
