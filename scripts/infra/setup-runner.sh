#!/usr/bin/env bash
# Register GitLab Runner on noedb-01 (115) — run from Proxmox host after GitLab is up.
set -euo pipefail

GITLAB_URL="${GITLAB_URL:-http://192.168.3.248}"
TOKEN="${1:?Usage: $0 <runner-registration-token>}"

pct exec 115 -- gitlab-runner register --non-interactive \
  --url "$GITLAB_URL" \
  --registration-token "$TOKEN" \
  --executor shell \
  --description "noedb-native-epyc" \
  --tag-list "noedb-native" \
  --run-untagged=false \
  --locked=false

pct exec 115 -- gitlab-runner verify
echo "Runner OK — tag: noedb-native"
