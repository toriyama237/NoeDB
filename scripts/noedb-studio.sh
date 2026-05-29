#!/usr/bin/env bash
# NoeDB Studio — professional launcher (demos, LinkedIn, talks).
#
#   ./scripts/noedb-studio.sh              # full-screen REPL
#   ./scripts/noedb-studio.sh --window     # new terminal window
#   ./scripts/noedb-studio.sh --cluster    # 3-node Raft REPL
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# ── ANSI palette ─────────────────────────────────────────────────────────────
if [[ -t 1 ]]; then
  ESC=$'\033'
  BOLD="${ESC}[1m" DIM="${ESC}[2m" RESET="${ESC}[0m"
  CYAN="${ESC}[1;36m" GRAY="${ESC}[90m" GREEN="${ESC}[1;32m"
else
  ESC="" BOLD="" DIM="" RESET="" CYAN="" GRAY="" GREEN=""
fi

VERSION="2.0.0"
[[ -f Cargo.toml ]] && VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')

print_startup() {
  clear 2>/dev/null || printf '%s[2J%s[H' "$ESC" "$ESC"

  printf '%s\n' \
    "${CYAN}${BOLD}" \
    "  ███╗   ██╗  ██████╗   ███████╗  ██████╗   ██████╗ " \
    "  ████╗  ██║ ██╔═══██╗ ██╔════╝  ██╔══██╗  ██╔══██╗" \
    "  ██╔██╗ ██║ ██║   ██║ █████╗    ██║  ██║  ██████╔╝" \
    "  ██║╚██╗██║ ██║   ██║ ██╔══╝    ██║  ██║  ██╔══██╗" \
    "  ██║ ╚████║ ╚██████╔╝ ███████╗  ██████╔╝  ██████╔╝" \
    "  ╚═╝  ╚═══╝  ╚═════╝  ╚══════╝  ╚═════╝   ╚═════╝ " \
    "${GRAY}  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀  ▀▀▀▀▀▀▀${RESET}" \
    "${RESET}"

  OK="${GREEN}[OK]${RESET}"
  ACTIVE="${GREEN}[Active]${RESET}"

  printf '%s\n' \
    "${GRAY}  ┌────────────────────────────────────────────────────────────────┐${RESET}" \
    "${GRAY}  │${RESET} Distributed SQL Engine ${GRAY}│${RESET} Rust 🦀 • Raft ⎈ • LSM 🗄️ • Made in CM 🇨🇲" \
    "${GRAY}  │${RESET} Core Modules: Lexer ${OK} • Parser ${OK} • Planner ${OK} • Storage ${ACTIVE} ${GRAY}│${RESET} Engine v${VERSION} ${OK}" \
    "${GRAY}  └────────────────────────────────────────────────────────────────┘${RESET}" \
    "${GRAY}  ┌─ [ QUICKSTART & COMMANDS ] ────────────────────────────────────┐${RESET}" \
    "${GRAY}  │${RESET} SELECT 1;                                    ${DIM}(basic query)${RESET}" \
    "${GRAY}  │${RESET} CREATE TABLE t (id TEXT); INSERT INTO t VALUES ('42');" \
    "${GRAY}  │${RESET} \\explain SELECT * FROM t;                  ${DIM}(query plan)${RESET}" \
    "${GRAY}  │${RESET} \\help                                       ${DIM}(show panel again)${RESET}" \
    "${GRAY}  └────────────────────────────────────────────────────────────────┘${RESET}" \
    ""

  steps=("Lexer" "Parser" "Planner" "Storage" "Raft")
  for i in "${!steps[@]}"; do
    name="${steps[$i]}"
    bar=""
    for ((b=1; b<=20; b++)); do
      if (( b <= (i + 1) * 4 )); then bar+="█"; else bar+="░"; fi
    done
    printf "  ${CYAN}[%s]${RESET} ${DIM}%-8s${RESET} boot…\r" "$bar" "$name"
    sleep 0.06
  done
  printf "\n  ${GREEN}✓${RESET} ${BOLD}All modules online${RESET}\n\n"
}

launch_inner() {
  export NOEDB_STUDIO=1
  export NOEDB_SKIP_BANNER=1
  export RUST_LOG="${RUST_LOG:-warn}"

  print_startup

  exec cargo run --release -p noedb-cli --quiet -- --studio "$@"
}

WINDOW=0
ARGS=()
for arg in "$@"; do
  case "$arg" in
    --window|-w) WINDOW=1 ;;
    *) ARGS+=("$arg") ;;
  esac
done

if (( WINDOW )); then
  if command -v gnome-terminal >/dev/null 2>&1; then
    exec gnome-terminal --title="NoeDB Studio" -- bash -lc "cd '$ROOT' && '$0' ${ARGS[*]}; echo; read -p 'Press Enter to close…'"
  elif command -v x-terminal-emulator >/dev/null 2>&1; then
    exec x-terminal-emulator -T "NoeDB Studio" -e bash -lc "cd '$ROOT' && '$0' ${ARGS[*]}; echo; read -p 'Press Enter…'"
  elif [[ "$(uname -s)" == "Darwin" ]] && command -v osascript >/dev/null 2>&1; then
    exec osascript -e "tell application \"Terminal\" to do script \"cd '$ROOT' && '$0' ${ARGS[*]}\""
  else
    echo "No terminal emulator found — running in this shell." >&2
  fi
fi

launch_inner "${ARGS[@]}"
