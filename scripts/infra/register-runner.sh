#!/usr/bin/env bash
# Register GitLab Runner on noedb-01 after GitLab CE is up.
# Usage: ./scripts/infra/register-runner.sh <registration-token>
set -euo pipefail

GITLAB_URL="${GITLAB_URL:-http://192.168.3.248}"
TOKEN="${1:?Usage: $0 <runner-registration-token>}"

ssh root@217.182.95.243 "pct exec 115 -- gitlab-runner register --non-interactive \
  --url '${GITLAB_URL}' \
  --registration-token '${TOKEN}' \
  --executor shell \
  --description 'noedb-native-epyc' \
  --tag-list 'noedb-native' \
  --run-untagged=false \
  --locked=false"

echo "Runner registered with tag: noedb-native"
