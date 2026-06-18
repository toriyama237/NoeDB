#!/usr/bin/env bash
# NoeDB stress suite — run inside LXC noedb-01 after release build.
set -euo pipefail

SRC="${NOEDB_SRC:-/opt/noedb/src}"
TARGET="${CARGO_TARGET_DIR:-/opt/noedb/target}"
DATA="${NOEDB_DATA_DIR:-/var/lib/noedb/stress-data}"
RECORDS="${NOEDB_STORM_RECORDS:-1000000}"

export CARGO_TARGET_DIR="$TARGET"
export RUSTFLAGS="${RUSTFLAGS:--C target-cpu=native}"
# shellcheck source=/dev/null
source /root/.cargo/env

cd "$SRC"

echo "=== build release CLI + insert_storm ==="
cargo build --release -p noedb-cli
cargo build --release -p noedb-engine --bench insert_storm
install -m 755 "$TARGET/release/noedb" /usr/local/bin/noedb

STORM_BIN=$(find "$TARGET/release/deps" -maxdepth 1 -name 'insert_storm-*' -executable -type f | head -1)
if [[ -z "$STORM_BIN" ]]; then
  echo "insert_storm binary not found under $TARGET/release/deps" >&2
  exit 1
fi

echo "=== YCSB engine workloads ==="
time cargo bench -p noedb-engine --bench ycsb -- --nocapture 2>&1 | tail -20

echo "=== SQL insert storm ($RECORDS rows) ==="
rm -rf "$DATA"
mkdir -p "$DATA"
time env NOEDB_DATA_DIR="$DATA" NOEDB_STORM_RECORDS="$RECORDS" "$STORM_BIN"

echo "=== done — monitor with: htop | iotop -o | iostat -xz 1 ==="
