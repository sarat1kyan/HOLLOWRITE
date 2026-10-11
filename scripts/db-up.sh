#!/usr/bin/env bash
# Starts PostgreSQL for development and makes sure the `hollowrite` role and
# the `hollowrite` / `hollowrite_test` databases exist. Tries, in order:
#   1. Docker Compose (service `db`),
#   2. a Debian/Ubuntu system cluster (pg_ctlcluster), as root or via sudo,
#   3. a private cluster in .data/pg using local PostgreSQL binaries
#      (not as root; initdb refuses to run as root).
set -euo pipefail
cd "$(dirname "$0")/.."

wait_ready() {
  for _ in $(seq 1 40); do
    "$@" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "PostgreSQL did not become ready" >&2
  return 1
}

if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  docker compose up -d db
  wait_ready docker compose exec -T db pg_isready -U hollowrite -d hollowrite
  echo "PostgreSQL (Docker) ready on 127.0.0.1:5432"
  exit 0
fi

bootstrap_sql="
do \$\$ begin
  if not exists (select from pg_roles where rolname = 'hollowrite') then
    create role hollowrite login password 'hollowrite' createdb;
  end if;
end \$\$;"

if command -v pg_lsclusters >/dev/null 2>&1; then
  as_postgres() {
    if [ "$(id -u)" = 0 ]; then runuser -u postgres -- "$@"; else sudo -u postgres "$@"; fi
  }
  read -r ver name _ < <(pg_lsclusters --no-header | sort -rn | head -1)
  if [ "$(id -u)" = 0 ]; then pg_ctlcluster "$ver" "$name" start 2>/dev/null || true
  else sudo pg_ctlcluster "$ver" "$name" start 2>/dev/null || true; fi
  wait_ready as_postgres pg_isready
  as_postgres psql -q -c "$bootstrap_sql"
  for db in hollowrite hollowrite_test; do
    as_postgres psql -tAc "select 1 from pg_database where datname = '$db'" | grep -q 1 \
      || as_postgres psql -q -c "create database $db owner hollowrite"
  done
  echo "PostgreSQL $ver/$name ready on 127.0.0.1:$(pg_lsclusters --no-header | awk -v v="$ver" -v n="$name" '$1==v && $2==n {print $3}')"
  exit 0
fi

if [ "$(id -u)" = 0 ]; then
  echo "running as root without Docker or a system PostgreSQL cluster; install PostgreSQL 16 or Docker" >&2
  exit 1
fi
pg_ctl_bin="$(command -v pg_ctl 2>/dev/null || ls /usr/lib/postgresql/*/bin/pg_ctl 2>/dev/null | tail -1 || true)"
if [ -z "$pg_ctl_bin" ]; then
  echo "neither Docker nor PostgreSQL binaries found; install one of them (see docs/SETUP.md)" >&2
  exit 1
fi
bindir="$(dirname "$pg_ctl_bin")"
data=.data/pg
if [ ! -d "$data" ]; then
  mkdir -p "$data"
  "$bindir/initdb" -D "$data" -U hollowrite --auth=trust >/dev/null
fi
"$bindir/pg_ctl" -D "$data" status >/dev/null 2>&1 || \
  "$bindir/pg_ctl" -D "$data" -l .data/pg.log -o "-p 5432 -k /tmp" start >/dev/null
wait_ready "$bindir/pg_isready" -h /tmp -p 5432
for db in hollowrite hollowrite_test; do
  "$bindir/psql" -h /tmp -p 5432 -U hollowrite -d postgres -tAc "select 1 from pg_database where datname = '$db'" | grep -q 1 \
    || "$bindir/psql" -h /tmp -p 5432 -U hollowrite -d postgres -q -c "create database $db"
done
"$bindir/psql" -h /tmp -p 5432 -U hollowrite -d postgres -q -c "alter role hollowrite password 'hollowrite'"
echo "private cluster in $data ready on 127.0.0.1:5432"
