#!/usr/bin/env bash
# NoeDB Studio — branded launcher for demos / LinkedIn / talks.
# Usage:
#   ./scripts/noedb-studio.sh              # full-screen REPL in this terminal
#   ./scripts/noedb-studio.sh --window     # new terminal window (Linux/macOS)
#   ./scripts/noedb-studio.sh --cluster    # 3-node Raft REPL
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ── colours ──────────────────────────────────────────────────────────────────
if [[ -t 1 ]] && command -v tput >/dev/null 2>&1; then
  ncolors=$(tput colors 2>/dev/null || echo 0)
else
  ncolors=0
fi
if [[ "$ncolors" -ge 8 ]]; then
  BOLD=$(tput bold); DIM=$(tput dim); RESET=$(tput sgr0)
  CYAN=$(tput setaf 6); GOLD=$(tput setaf 3); GREEN=$(tput setaf 2)
else
  BOLD=""; DIM=""; RESET=""; CYAN=""; GOLD=""; GREEN=""
fi

VERSION="2.0.0"
[[ -f Cargo.toml ]] && VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

launch_inner() {
  export NOEDB_STUDIO=1
  export NOEDB_CLEAR=1
  export RUST_LOG="${RUST_LOG:-warn}"

  clear 2>/dev/null || printf '\033[2J\033[H'

  cat <<EOF
${CYAN}${BOLD}
    ███╗   ██╗ ██████╗ ███████╗██████╗ ██████╗
    ████╗  ██║██╔═══██╗██╔════╝██╔══██╗██   ██╗
    ██╔██╗ ██║██║   ██║█████╗  ██║  ██║██   ██║
    ██║╚██╗██║██║   ██║██╔══╝  ██║  ██║██   ██║
    ██║ ╚████║╚██████╔╝███████╗██████╔╝██████╔╝
    ╚═╝  ╚═══╝ ╚═════╝ ╚══════╝╚═════╝ ╚═════╝
${RESET}${DIM}  ─────────────────────────────────────────────────────────────
  ${GOLD}NoeDB Studio${RESET}${DIM} · distributed SQL · Rust · Raft · LSM · v${VERSION}
  ─────────────────────────────────────────────────────────────${RESET}

EOF

  steps=(
    "noedb-lexer   tokenizing SQL surface"
    "noedb-parser  building AST"
    "noedb-planner cost-based optimizer"
    "noedb-storage LSM + MVCC online"
    "noedb-raft    consensus ready"
  )
  i=0
  for step in "${steps[@]}"; do
    i=$((i + 1))
    name="${step%% *}"
    desc="${step#* }"
    bar=""
    for ((b=1; b<=20; b++)); do
      if (( b <= i * 4 )); then bar+="█"; else bar+="░"; fi
    done
    printf "  ${CYAN}[%s]${RESET} ${DIM}%-14s${RESET} %s\r" "$bar" "$name" "$desc"
    sleep 0.08
  done
  printf "\n  ${GREEN}✓${RESET} ${BOLD}Engine online${RESET} — dropping into SQL shell…\n\n"
  sleep 0.25

  # Prefer release binary when built; fall back to dev build.
  if [[ -x "$ROOT/target/release/noedb" ]]; then
    exec "$ROOT/target/release/noedb" --studio "$@"
  fi
  exec cargo run -q -p noedb-cli -- --studio "$@"
}

# ── optional new window ──────────────────────────────────────────────────────
WINDOW=0
ARGS=()
for arg in "$@"; do
  if [[ "$arg" == "--window" || "$arg" == "-w" ]]; then
    WINDOW=1
  else
    ARGS+=("$arg")
  fi
done

if (( WINDOW )); then
  if command -v gnome-terminal >/dev/null 2>&1; then
    exec gnome-terminal --title="NoeDB Studio" -- bash -lc "cd '$ROOT' && '$0' ${ARGS[*]}; echo; read -p 'Press Enter to close…'"
  elif command -v x-terminal-emulator >/dev/null 2>&1; then
    exec x-terminal-emulator -T "NoeDB Studio" -e bash -lc "cd '$ROOT' && '$0' ${ARGS[*]}; echo; read -p 'Press Enter…'"
  elif [[ "$(uname -s)" == "Darwin" ]] && command -v osascript >/dev/null 2>&1; then
    exec osascript -e "tell application \"Terminal\" to do script \"cd '$ROOT' && '$0' ${ARGS[*]}\""
  else
    echo "No supported terminal emulator — running in this shell." >&2
  fi
fi

launch_inner "${ARGS[@]}"
