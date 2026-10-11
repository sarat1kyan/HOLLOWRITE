#!/usr/bin/env bash
# Reproduces the cross-client combat capture: a terminal player (tmux) and a
# browser player (headless Chromium) spawn at the Last Lantern, walk the same
# route to the first Oathless Knight in the Lampless Streets and fight it.
#
#   scripts/cross-client-fight.sh <terminal-name> <browser-name> [outdir]
#
# Needs a running server on :7700, the Vite dev server on :5173, tmux and
# Playwright. Use fresh names so both start at the lantern.
set -euo pipefail
cd "$(dirname "$0")/.."
tname=${1:?terminal player name}; bname=${2:?browser player name}; out=${3:-captures}
mkdir -p "$out"
cfg=$(mktemp -d)
cargo build -q --release -p hollowrite-terminal
tmux kill-session -t hw-fight 2>/dev/null || true
tmux new-session -d -s hw-fight -x 120 -y 35 "env COLORTERM=truecolor XDG_CONFIG_HOME=$cfg target/release/hollowrite"
sleep 1.5
tmux send-keys -t hw-fight F2; tmux send-keys -t hw-fight -l "$tname"; tmux send-keys -t hw-fight Tab
tmux send-keys -t hw-fight -l "fight-scene-pass"; tmux send-keys -t hw-fight Enter; sleep 2
tmux send-keys -t hw-fight -l "$tname"; tmux send-keys -t hw-fight Enter; sleep 1.5
tmux send-keys -t hw-fight Enter; sleep 2

# route from the lantern: north 1, east 21, north 7, west 5, north 6, east 3
route="w:0.28 d:5.83 w:1.94 a:1.39 w:1.67 d:0.6"
hold() { # key seconds: keep the emulated hold alive with 90 ms repeats
  local key=$1 end; end=$(python3 -c "import time;print(time.time()+$2-0.14)")
  while python3 -c "import sys,time;sys.exit(0 if time.time()<$end else 1)"; do
    tmux send-keys -t hw-fight "$key"; sleep 0.09
  done
  sleep 0.2
}
browser_route="KeyW:280,KeyD:5830,KeyW:1940,KeyA:1390,KeyW:1670,KeyD:600"
FIGHT_MS=9000 node scripts/browser-session.mjs http://127.0.0.1:5173 "$bname" fight-scene-pass "$out" "$browser_route" > "$out/browser-fight.json" &
bpid=$!
sleep 4
for step in $route; do hold "${step%%:*}" "${step##*:}"; done
for _ in $(seq 1 14); do tmux send-keys -t hw-fight j; sleep 0.6; done
tmux capture-pane -t hw-fight -e -p > "$out/terminal-fight.ans"
python3 scripts/ansi2html.py "$out/terminal-fight.ans" > "$out/terminal-fight.html"
wait $bpid || true
echo "captures in $out (terminal-fight.*, web-$bname-*.png)"
