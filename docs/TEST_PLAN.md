# Test plan and results

Everything below was run on 2026-10-11 in a 4-core Linux development
container (PostgreSQL 16.15 local, Rust 1.97.0, Node 22.22, Chromium via
Playwright 1.56). Numbers come from that machine; they are not capacity
promises for any other one.

## How to run

| command | what | needs |
|---|---|---|
| `cargo test --workspace` | everything; database tests skip themselves without `TEST_DATABASE_URL` | — |
| `make test-db` | persistence and end-to-end server tests | PostgreSQL, `TEST_DATABASE_URL` |
| `make test-web` | TypeScript typecheck and Vitest | Node |
| `make lint` | rustfmt, clippy `-D warnings`, eslint | |
| `make smoke` / `load` / `latency` | protocol bots against a running server | running server |
| `HOLLOWRITE_WRITE_FIXTURES=1 cargo test -p hollowrite-protocol` | regenerate Rust→TS protocol fixtures | |

CI (`.github/workflows/ci.yml`) runs fmt, clippy, migrations from an empty
database, content validation, all Rust tests against a PostgreSQL service,
a smoke run against the built server, the web typecheck/lint/test/build, and
`cargo audit` / `npm audit`.

## Suites and what they protect

### Simulation (`crates/game-core/tests/simulation.rs`, 26 tests)

Deterministic: seeded world, explicit `step()`, no sleeps.

- diagonal speed equals orthogonal speed; walls cannot be tunnelled by
  walking or rolling; movement is locked during windup/active/recovery
- actions need stamina and stamina never underflows
- roll invulnerability exists exactly on the configured frames
- light attacks respect windup, reach and line of sight; attacks do not pass
  through walls
- a knight telegraphs at least 15 ticks before its hit lands
- parry inside the window staggers and enables a critical riposte; an early
  parry fails
- enemies leash home and heal; kill rewards pay each contributor exactly
  once
- death drops a remnant, respawn happens at the checkpoint, reclaiming on two
  consecutive ticks pays once; dying again before reclaiming loses the old
  remnant and reports it
- stale and duplicate input sequence numbers are ignored
- map pickups are awarded once per character, including two characters on
  the same tick
- the key gate needs the key; one-way shortcuts open only from their side
- level-up needs a lantern and enough echoes and charges correctly
- healing is committed (no heal before the drink lands)
- boss scales with participants, changes phase at 50% with invulnerable
  transition, resets when everyone dies, launches shockwave rings followed
  by a vulnerability window, rewards participants exactly once
- equipment validates slot, ownership and attribute requirements
- interest management hides far entities and reports them as gone
- the whole simulation is reproducible for a seed

### Content (`content.rs`, 6) and playthrough (`playthrough.rs`, 3)

Reachability of every point of interest; gates and the secret actually gate;
the tunnel shortcut is shorter and safe; required features exist; map border
is closed; no enemies in the sanctuary; dialogue rewards cannot be farmed;
level costs grow. The playthrough tests walk a character with inputs only
through both quests (lanterns, key, gate, Ossuary shortcut, boss, ending
choice), the secret grove (false wall, Penitent Maul, Moth's once-only gift)
and the tunnel shortcut (crossroads to plaza in under 10 s).

### Balance (`balance.rs`, 2)

Scripted duels against each regular enemy for two origins over six seeds,
plus an idle player against a knight. Current results are in
CONTENT_MANIFEST.md ("Measured fight lengths"). These duels found and drove
fixes for a stun-lock, an endless enemy attack chain and kiting enemies.

### Protocol (`crates/protocol`, 8)

Tile codes unique and round-trip; sanitising removes escape sequences,
bidi and zero-width characters and clamps length; names validated (including
a Cyrillic homoglyph); message shapes; unknown variants rejected; web client
messages deserialize and round-trip; Rust-generated fixtures match the
committed file.

### Persistence (`crates/persistence/tests/db.rs`, 8, real PostgreSQL)

Case-insensitive unique names; session create/resolve/revoke/expire;
suspension revokes sessions, blocks new ones and is audited; a character
aggregate round-trips exactly and is scoped to its account; ledger rows are
written; stale and replayed saves are rejected; 20 concurrent saves of one
character converge on the newest version; the four-character limit holds
under 10 concurrent creations; database constraints reject negative echoes,
a half-written remnant and invalid names.

### Server end to end (`crates/server/tests/multiplayer.rs`, 6, real HTTP + WebSocket + PostgreSQL)

- two players in one world see each other and agree on authoritative
  positions after movement
- local chat is delivered and stripped of escape sequences; party chat
  requires a party; invite/accept/leave; party relation shows in snapshots;
  chat flooding is rate-limited
- hostile traffic: selecting another account's character, invented message
  types, extra fields, duplicate and stale inputs, using unowned items, and
  oversized frames are all refused (the last closes the socket)
- a second login takes over the character and the first client is told
- a consumed item and the position survive disconnect and a full server
  restart
- HTTP surface: CSRF header required, foreign origins refused, name and
  password validation, `HttpOnly`/`SameSite=Strict` cookie and no token in
  browser responses, wrong password 401, WebSocket without credentials 401,
  with a foreign origin 403, cookie without origin 401, logout revokes,
  health/readiness/metrics respond

### Clients

- terminal (`crates/terminal-client`, 7): theme covers every tile, every
  glyph is single-width, ASCII mode is pure ASCII, 256-colour quantising,
  doors block sight and movement until opened, interpolation bounds, display
  sanitising
- web (`apps/web`, 12 Vitest): Rust-generated fixtures drive the client
  model end to end, text cleaning and name validation, door visibility and
  passability, interpolation, prediction never enters walls, the per-cell
  glyph hash matches the terminal client's, every tile code is styled, ASCII
  fallbacks are ASCII, held keys combine into diagonals, action keys ignore
  repeat and guard pairs on/off

## Results of the last full run

```
game-core   43 passed  (lib 6, balance 2, content 6, playthrough 3, simulation 26)
protocol     8 passed
persistence  8 passed  (PostgreSQL)
server       8 passed  (unit 2, end-to-end 6 with PostgreSQL)
terminal     7 passed
web         12 passed  (plus tsc and eslint clean)
clippy -D warnings: clean    rustfmt: clean    cargo audit: clean (one documented ignore)    npm audit: 0 vulnerabilities
```

## Cross-client walkthrough (acceptance scene)

Performed against the real binaries (debug server, Vite dev server, terminal
client in a 120×35 tmux pane, Chromium driven by
`scripts/browser-session.mjs`):

| step | result | evidence |
|---|---|---|
| A. start database, server and web app from the README commands | done | — |
| B. register Alice in the terminal and Bob in the browser | done through each client's own UI | — |
| C. both create characters and meet at the Last Lantern | done | `docs/captures/*` |
| D. both see each other move; chat crosses clients | done; Bob's message appears in Alice's terminal | `docs/captures/terminal-alice-sees-bob.png`, `browser-bob-sees-alice.png` |
| D. party | verified by the end-to-end test and `make smoke`, not by hand in the two UIs | test output |
| E. shared enemy, telegraphs, rewards | done: terminal Dave and browser Erin walked the same route and killed the first knight together; both at the same health (296/444), both +60 echoes once, Dave's Grave Salt drop personal | `docs/captures/terminal-dave-fights-knight.png`, `browser-erin-fights-knight.png`, `scripts/cross-client-fight.sh` |
| F. same boss health and phase for both | done: terminal Hank and browser Ivy entered the belfry together; the door sealed for both, both took the same sweep (173/840) and both boss bars show the same health. The two characters were staged next to the Sexton's Lantern with SQL (position, lantern and gate flags, Vitality 30) to skip the walk; everything after that went through the clients | `docs/captures/terminal-hank-bell-mother.png`, `browser-ivy-bell-mother.png` |
| G. death, respawn, remnant retrieval | verified in simulation; the persistence of remnants verified in the database round-trip test | tests |
| H. disconnect, restart, resume | verified by the end-to-end restart test (position and inventory) | test output |
| I. low-colour / ASCII mode | done: `--ascii --colors 16` at 100×30, all terrain, players, NPC and lantern legible | `docs/captures/terminal-ascii-16colour.png` |

Steps marked "not yet captured" are honest gaps in manual evidence, not in
functionality: the server code paths are the same for both clients, and the
tests exercise them through the real protocol.

Layout captures: `docs/captures/terminal-80x24.png` (compact layout) and
`docs/captures/terminal-ascii-16colour.png` (`--ascii --colors 16`).

## Performance

Release build, server and bots on the same 4-core container (the bots
compete with the server for CPU).

| scenario | result |
|---|---|
| 20 bots wandering 30 s (`load --bots 20`) | 20/20 ok; tick last 98 µs, max 1.5 ms (budget 33 ms), 0 overruns; 10 363 snapshots, mean 5.3 KB, 0 dropped |
| 60 bots wandering 30 s, all spawning in the sanctuary | 60/60 ok; tick max 16.6 ms, 0 overruns; 30 175 snapshots, mean 13.0 KB, 0 dropped; server RSS 64 MB |
| latency, no added delay | ping 2 ms; input→authoritative ack p50 66 ms, p95 67 ms |
| latency, 150 ms RTT ±20 ms one-way jitter (`LatencyProxy`) | ping p50 172 ms; input→ack p50 201 ms, p95 222 ms, max 242 ms |
| latency, 200 ms RTT ±30 ms one-way jitter | ping p50 236 ms; input→ack p50 265 ms, p95 292 ms, max 313 ms |

Interpretation:

- Input-to-ack is RTT plus up to one tick plus up to one snapshot interval
  (66 ms). Local movement prediction hides this for walking; attacks and
  rolls are shown when the server confirms them.
- Snapshot size grows with crowding because v0 sends full entity views for
  everything in a 34-cell radius. 60 players in one room is about 11.7 MB/s
  of outbound JSON; field deltas and a binary codec are the first
  optimisation if real numbers demand it.
- No test here establishes a player cap for a production machine.

## Backup and restore drill

`pg_dump --format=custom` of the development database (27 accounts, 27
characters), restored with `pg_restore --no-owner` into a fresh database;
row counts matched, `migrate` and `check-content` passed against it, and a
release server on the restored database passed `hollowrite-bot smoke`, then
shut down cleanly on SIGTERM.

## Not covered yet

- Windows and macOS terminals and Windows console quirks were not tested
  here (Linux only). Crossterm supports them; this needs real machines.
- Terminal resize stress and terminals without kitty keyboard support were
  exercised through tmux only (tmux reports no key releases, so repeat mode
  was what got tested).
- Browser coverage is Chromium only. Firefox and Safari were not run.
- The Docker image and Compose `full` profile were written but not built
  here (no Docker daemon in this environment).
- No human playtests; the 30–60 minute session estimate is unvalidated.
