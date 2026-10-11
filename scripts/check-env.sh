#!/usr/bin/env bash
# Reports what is installed and reachable. Exits non-zero if something
# required is missing.
set -u
ok=1
need() { if command -v "$1" >/dev/null 2>&1; then printf '  ok    %-10s %s\n' "$1" "$($1 --version 2>&1 | head -1)"; else printf '  MISS  %-10s %s\n' "$1" "$2"; ok=0; fi; }
want() { if command -v "$1" >/dev/null 2>&1; then printf '  ok    %-10s %s\n' "$1" "$( { $1 --version || $1 -V; } 2>/dev/null | head -1)"; else printf '  --    %-10s %s\n' "$1" "$2"; fi; }
echo "toolchain"
need cargo "install Rust from https://rustup.rs (stable, 1.87+)"
need node "install Node.js 20 or 22"
need npm "comes with Node.js"
want docker "optional: used by 'make db-up' for PostgreSQL"
want psql "optional: PostgreSQL client"
want tmux "optional: terminal captures"
echo "database"
url="${DATABASE_URL:-postgres://hollowrite:hollowrite@127.0.0.1:5432/hollowrite}"
if command -v pg_isready >/dev/null 2>&1; then
  if pg_isready -d "$url" >/dev/null 2>&1; then echo "  ok    reachable at ${url%%@*}@…"; else echo "  --    not reachable; run 'make db-up'"; fi
else
  echo "  --    pg_isready not installed; skipping check"
fi
echo "ports"
for p in 7700 5173; do
  if (exec 3<>/dev/tcp/127.0.0.1/$p) 2>/dev/null; then echo "  busy  $p (something is already listening)"; else echo "  free  $p"; fi
done
[ "$ok" = 1 ] || { echo "missing required tools"; exit 1; }
