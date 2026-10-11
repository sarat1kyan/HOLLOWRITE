#!/usr/bin/env bash
# Runs the game server and the Vite dev server; Ctrl-C stops both.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -d apps/web/node_modules ] || (cd apps/web && npm ci --no-audit --no-fund)
cargo build -q -p hollowrite-server
trap 'kill 0' EXIT INT TERM
cargo run -q -p hollowrite-server &
(cd apps/web && npm run dev -- --host 127.0.0.1) &
echo "server  http://127.0.0.1:7700   web  http://127.0.0.1:5173   terminal  make dev-terminal"
wait
