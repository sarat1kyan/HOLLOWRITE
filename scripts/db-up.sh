#!/usr/bin/env bash
# Starts PostgreSQL for development: Docker Compose when available,
# otherwise a throwaway cluster in .data/pg using the local PostgreSQL
# binaries (useful in containers without a Docker daemon).
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  docker compose up -d db
  echo "waiting for PostgreSQL..."
  for _ in $(seq 1 40); do
    docker compose exec -T db pg_isready -U hollowrite -d hollowrite >/dev/null 2>&1 && { echo "ready"; exit 0; }
    sleep 1
  done
  echo "PostgreSQL did not become ready" >&2
  exit 1
fi
bindir="$(dirname "$(command -v pg_ctl 2>/dev/null || ls /usr/lib/postgresql/*/bin/pg_ctl 2>/dev/null | tail -1)")"
if [ ! -x "$bindir/pg_ctl" ]; then
  echo "neither Docker nor PostgreSQL binaries are available; install one of them" >&2
  exit 1
fi
data=.data/pg
if [ ! -d "$data" ]; then
  mkdir -p "$data"
  "$bindir/initdb" -D "$data" -U hollowrite --auth=trust >/dev/null
fi
"$bindir/pg_ctl" -D "$data" -l .data/pg.log -o "-p 5432 -k /tmp" status >/dev/null 2>&1 || \
  "$bindir/pg_ctl" -D "$data" -l .data/pg.log -o "-p 5432 -k /tmp" start >/dev/null
sleep 1
for db in hollowrite hollowrite_test; do
  "$bindir/psql" -h /tmp -p 5432 -U hollowrite -d postgres -tc "select 1 from pg_database where datname='$db'" | grep -q 1 || \
    "$bindir/psql" -h /tmp -p 5432 -U hollowrite -d postgres -c "create database $db" >/dev/null
done
"$bindir/psql" -h /tmp -p 5432 -U hollowrite -d postgres -c "alter role hollowrite password 'hollowrite'" >/dev/null
echo "local cluster running in $data on port 5432"
