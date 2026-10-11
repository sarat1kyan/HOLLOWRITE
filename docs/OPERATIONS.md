# Operations

## Processes and ports

| process | default address | notes |
|---|---|---|
| `hollowrite-server` | `127.0.0.1:7700` | HTTP API, WebSocket `/ws`, health, metrics, optional static web bundle |
| Vite dev server | `127.0.0.1:5173` | development only; proxies `/api` and `/ws` to the game server |
| PostgreSQL 16 | `127.0.0.1:5432` | databases `hollowrite` and `hollowrite_test` |

Configuration is environment variables only; `.env.example` lists all of
them with defaults. The server reads `.env` only through `make`.

## Commands

```bash
hollowrite-server               # serve (default)
hollowrite-server migrate       # apply migrations and exit
hollowrite-server check-content # validate content and exit
hollowrite-server admin suspend <name> --hours 24 --reason "..."
hollowrite-server admin unsuspend <name>
hollowrite-server admin promote <name> [--revoke]
hollowrite-server admin reports
```

The server also runs migrations on start, so a fresh database becomes
playable with no extra steps. There are no seeded accounts.

## Health and metrics

- `GET /healthz`: the process is up.
- `GET /readyz`: 200 only if the world actor ticked in the last 2 seconds and
  the database answers within 2 seconds; 503 otherwise, with a JSON body
  saying which part failed.
- `GET /metrics`: Prometheus text, served only to loopback unless
  `HOLLOWRITE_PUBLIC_METRICS=1`. Metrics: connections, players online, ticks,
  last and maximum tick duration, tick overruns, snapshot bytes/sent/dropped,
  frames in/rejected, slow-consumer disconnects, saves ok/stale/failed, last
  database save latency.

## Logs

Structured `tracing` logs on stdout. `HOLLOWRITE_LOG_FORMAT=json` for JSON
lines; `RUST_LOG` filters (`info,sqlx=warn` by default). Connection logs carry
`conn` ids and account ids; world logs carry character names and entity
ids; chat is logged with sender, channel and length but not content;
moderation events use the `moderation` target. No passwords or tokens are
logged.

## Shutdown and restart

On SIGINT or SIGTERM the server:

1. tells every player it is shutting down and closes their sockets,
2. saves every online character,
3. stops accepting connections and drains the HTTP server,
4. waits for the persistence queue to flush (up to 10 seconds),
5. exits.

Docker Compose gives the container 20 seconds. Enemies, ground loot, parties
and boss encounters reset on restart by design; characters resume from their
last save at their saved position (moved to their checkpoint if that position
is inside the boss arena or no longer walkable).

### After an unclean exit

Nothing needs repairing: every save is a single transaction, so the database
holds the last committed aggregate for each character. Players lose at most
the progress since their last save (30 seconds of movement and kill rewards,
less for anything that triggers an immediate save). Restart the server
normally.

## Database

- Migrations live in `db/migrations` and are embedded in the server binary.
  Add a new numbered file for every change; never edit an applied migration.
- Rollback: migrations are forward-only. To undo one, write a new migration
  that reverses it, or restore a backup taken before the upgrade.

### Backup and restore

```bash
# backup (consistent snapshot; safe while the server runs)
pg_dump --format=custom --dbname "$DATABASE_URL" --file hollowrite-$(date +%F).dump

# restore into an empty database
createdb hollowrite_restore
pg_restore --no-owner --dbname postgres://hollowrite:hollowrite@127.0.0.1:5432/hollowrite_restore hollowrite-YYYY-MM-DD.dump
DATABASE_URL=postgres://hollowrite:hollowrite@127.0.0.1:5432/hollowrite_restore hollowrite-server check-content
```

The walkthrough above was executed against the local cluster while writing
this document (see TEST_PLAN.md, "Backup and restore").

## Low-cost single VPS deployment (recommended start)

One small Linux VM running:

- PostgreSQL 16 (same host is fine at this size; managed Postgres if you
  prefer someone else to handle backups),
- `hollowrite-server` as a systemd service or the Docker image,
- Caddy or nginx in front for TLS, forwarding `/` and `/ws` with WebSocket
  upgrade headers, and HTTP/2 or HTTP/1.1 to the backend.

Settings for that setup: `HOLLOWRITE_BIND=127.0.0.1:7700`,
`HOLLOWRITE_SECURE_COOKIES=1`,
`HOLLOWRITE_ALLOWED_ORIGINS=https://your.domain`, a strong database password,
and nightly `pg_dump` copied off the machine.

Sizing has to come from load tests on the actual machine. What was measured
here (TEST_PLAN.md) is a single development container; the simulation of the
whole world used well under one CPU core with 20 connected bots. Do not plan
around player counts that have not been measured.

Tradeoffs: one VM is cheap and simple but a single point of failure;
restarts disconnect everyone for a few seconds. That is acceptable for an
early community.

## Later architecture

When measured load demands it: separate PostgreSQL host or managed service,
one world process per zone behind a connection gateway, party instances for
boss fights, and a shared bus (Redis or NATS) only once cross-process state
actually exists. ARCHITECTURE.md describes where those seams already are.
Kubernetes is not needed for any of this.
