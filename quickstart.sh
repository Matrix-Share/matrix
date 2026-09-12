#!/usr/bin/env bash
# Lifeline quickstart — one command to a running two-person mesh demo.
#
#   ./quickstart.sh
#
# Prefers Docker (nothing to install but Docker); falls back to a Rust build.
# Opens two browser tabs, Asha and Ravi, that auto-discover each other over a
# local zero-knowledge relay. Type a message in one; it arrives E2E-encrypted in
# the other with a signed delivery receipt.
#
# NOTE (alpha, be honest): this demo meshes over a local relay/LAN that stands in
# for the internet transport. The native phone-to-phone Bluetooth/Wi-Fi Aware
# radios are built but not yet shipped, and Lifeline has not had a security audit
# — so don't rely on it for a real emergency yet. See README.md.
set -euo pipefail

ASHA="http://localhost:8080"
RAVI="http://localhost:8081"
cd "$(dirname "$0")"

say()  { printf '\033[1m%s\033[0m\n' "$*"; }
info() { printf '  %s\n' "$*"; }

open_tabs() {
  local opener=""
  if command -v open        >/dev/null 2>&1; then opener="open";        # macOS
  elif command -v xdg-open  >/dev/null 2>&1; then opener="xdg-open";     # Linux
  fi
  if [ -n "$opener" ]; then
    ( sleep "$1"; "$opener" "$ASHA" >/dev/null 2>&1 || true
                  "$opener" "$RAVI" >/dev/null 2>&1 || true ) &
  fi
}

banner() {
  say ""
  say "  Lifeline demo mesh is starting."
  info "Asha  →  $ASHA"
  info "Ravi  →  $RAVI"
  info "They auto-discover each other. Type a message in one tab; it arrives"
  info "encrypted in the other with a signed delivery receipt."
  say ""
}

# ---- Path 1: Docker (self-contained) ---------------------------------------
if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
  say "==> Using Docker (relay + two nodes)."
  banner
  open_tabs 12
  exec docker compose up --build
fi

# ---- Path 2: from source (Rust) --------------------------------------------
if command -v cargo >/dev/null 2>&1; then
  say "==> Docker not found; building from source with cargo."
  info "First build can take a few minutes; later runs are instant."
  say ""

  pids=()
  cleanup() { say ""; say "==> Shutting down the demo mesh."; for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null || true; done; }
  trap cleanup INT TERM EXIT

  say "==> Starting the zero-knowledge relay on :7000"
  cargo run -q -p lifeline-relay & pids+=($!)
  sleep 2

  say "==> Starting node 'Asha' on :8080"
  LIFELINE_NODE_ADDR=127.0.0.1:8080 LIFELINE_NAME=Asha cargo run -q -p lifeline-node & pids+=($!)
  say "==> Starting node 'Ravi' on :8081"
  LIFELINE_NODE_ADDR=127.0.0.1:8081 LIFELINE_NAME=Ravi cargo run -q -p lifeline-node & pids+=($!)

  banner
  open_tabs 6
  info "Press Ctrl+C to stop."
  wait
  exit 0
fi

# ---- Neither available ------------------------------------------------------
say "Lifeline needs either Docker or the Rust toolchain to run the demo."
info "Install Docker:  https://docs.docker.com/get-docker/   (then re-run ./quickstart.sh)"
info "Or install Rust: https://rustup.rs                     (then re-run ./quickstart.sh)"
exit 1
