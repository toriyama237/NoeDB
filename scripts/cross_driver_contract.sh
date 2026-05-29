#!/usr/bin/env bash
# Phase 7 cross-driver contract: same auth + SELECT 1 shape (requires running gRPC server).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA_DIR="${NOEDB_DATA_DIR:-/tmp/noedb-cross-driver}"
TARGET="${NOEDB_TARGET:-127.0.0.1:5434}"

if [[ ! -f "${DATA_DIR}/tls/ca.pem" ]]; then
  echo "Missing ${DATA_DIR}/tls/*.pem — start the server first:"
  echo "  cargo run -p noedb-cli -- --server --data-dir ${DATA_DIR}"
  exit 1
fi

echo "=== Python ==="
(
  cd "$ROOT/clients/python"
  pip install -q -e . 2>/dev/null || pip install -e .
  python3 -c "
from noedb import NoeDbClient
c = NoeDbClient.from_dev_certs('${TARGET}', '${DATA_DIR}')
c.ping()
r = c.execute('SELECT 1')
assert r.rows, r
print('python ok', r.rows)
c.close()
"
)

echo "=== Go ==="
(
  cd "$ROOT/clients/go"
  go run ./cmd/smoke/main.go -target "$TARGET" -data "$DATA_DIR"
)

echo "=== Node ==="
(
  cd "$ROOT/clients/nodejs"
  npm install --silent
  node --input-type=module -e "
import { NoeDbClient } from './src/index.js';
const c = NoeDbClient.fromDevCerts('${TARGET}', '${DATA_DIR}');
await c.ping();
const r = await c.execute('SELECT 1');
console.log('node ok', r.rows);
c.close();
"
)

echo "cross-driver contract: OK"
