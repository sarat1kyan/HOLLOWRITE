#!/usr/bin/env bash
# Starts the terminal client in a detached tmux session of a given size and
# writes an ANSI capture plus an HTML rendering of it.
#
#   scripts/capture-terminal.sh [cols] [rows] [out-prefix]
#
# Drive it with `tmux send-keys -t hw-capture ...` between start and capture,
# or let it sit on the login screen. Set HOLD=seconds to wait before capturing.
set -euo pipefail
cd "$(dirname "$0")/.."
cols=${1:-120}; rows=${2:-35}; out=${3:-captures/terminal-${cols}x${rows}}
mkdir -p "$(dirname "$out")"
cargo build -q -p hollowrite-terminal
tmux kill-session -t hw-capture 2>/dev/null || true
tmux new-session -d -s hw-capture -x "$cols" -y "$rows" "env COLORTERM=truecolor target/debug/hollowrite ${CLIENT_ARGS:-}"
sleep "${HOLD:-2}"
tmux capture-pane -t hw-capture -e -p > "$out.ans"
python3 scripts/ansi2html.py "$out.ans" > "$out.html"
echo "wrote $out.ans and $out.html (session hw-capture left running)"
