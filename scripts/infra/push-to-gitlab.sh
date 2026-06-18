#!/usr/bin/env bash
# Point your ThinkPad at GitLab OVH and push NoeDB (GitHub no longer required).
set -euo pipefail

GITLAB_HOST="${GITLAB_HOST:-192.168.3.248}"
GITLAB_URL="http://${GITLAB_HOST}"
PROJECT="${GITLAB_PROJECT:-root/noedb}"

echo "Requires VPN/tunnel to LAN 192.168.3.0/24, or run from Proxmox host."
echo "Example tunnel: ssh -L 8480:192.168.3.248:80 root@217.182.95.243"
echo ""

cd "$(git rev-parse --show-toplevel)"

if ! git remote get-url gitlab >/dev/null 2>&1; then
  git remote add gitlab "${GITLAB_URL}/${PROJECT}.git"
fi

git push gitlab --all
git push gitlab --tags 2>/dev/null || true

echo "Pushed to ${GITLAB_URL}/${PROJECT}"
