#!/usr/bin/env bash
# Push NoeDB to GitLab OVH sandbox. ThinkPad = git only; runner compiles.
set -euo pipefail

GITLAB_HTTP="${GITLAB_HTTP:-http://127.0.0.1:8480}"
GITLAB_SSH="${GITLAB_SSH:-ssh://git@127.0.0.1:2229}"
PROJECT="${GITLAB_PROJECT:-root/noedb}"

echo "Tunnel required from outside LAN:"
echo "  ssh -N -L 8480:192.168.3.249:80 -L 2229:192.168.3.249:2222 root@217.182.95.243"
echo ""

cd "$(git rev-parse --show-toplevel)"

if ! git remote get-url gitlab >/dev/null 2>&1; then
  git remote add gitlab "${GITLAB_HTTP}/${PROJECT}.git"
fi

git push gitlab --all
git push gitlab --tags 2>/dev/null || true

echo "GitLab: ${GITLAB_HTTP}/${PROJECT}"
