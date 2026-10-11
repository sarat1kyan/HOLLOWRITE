# Architecture

HOLLOWRITE is a **modular monolith**: one server process owns one logical
world. Gameplay is decided in exactly one place, the world actor, and every
client is a view of it.

```
   terminal client                        browser client
 (Ratatui/Crossterm)                (React screens + Canvas 2D)
        │  Bearer token                     │  HttpOnly cookie + Origin
        │  WebSocket JSON                   │  WebSocket JSON
        └──────────────┬────────────────────┘
                       ▼
 ┌──────────────────────── hollowrite-server ────────────────────────────┐
 │  Axum HTTP: /api/register /api/login /api/logout /api/me              │
 │             /healthz /readyz /metrics   static web bundle (optional)  │
 │                                                                       │
 │  per-connection task (ws.rs)                                          │
 │    hello/version check → lobby (list/create/select) → in world        │
 │    frame size cap, rate limit, malformed-frame strikes, idle timeout  │
 │         │ bounded mpsc (4096)                 ▲ bounded mpsc (512)     │
 │         ▼                                     │ per connection         │
 │  world actor (game.rs) ── single task, owns everything below ─────────┤
 │    hollowrite_game_core::World  (30 Hz fixed step, seeded RNG)        │
 │    connections, parties, invites, chat routing, mutes                 │
 │    snapshot per player every 2 ticks (interest radius 34 cells)       │
 │         │ Out::Save(aggregate + ledger)                               │
 │         ▼ unbounded FIFO                                              │
 │  persistence actor (persist.rs) ── single writer, also serves loads   │
 │         │ sqlx, one transaction per save                              │
 └─────────┼─────────────────────────────────────────────────────────────┘
           ▼
     PostgreSQL 16   accounts · sessions · characters (+items, flags,
                     lanterns) · echo_ledger · audit_log · mutes · reports
```

## Crates and dependency direction

```
protocol  ◄── game-core ◄── persistence ◄── server
    ▲             ▲                           │
    │             └───────────────────────────┘
    ├── terminal-client
    └── test-support            apps/web mirrors protocol in TypeScript
```

- **protocol** has no project dependencies. It is the shared vocabulary:
  message enums, tiles, directions, archetypes, validation and sanitising.
- **game-core** depends only on protocol, serde and toml. No async runtime,
  no sockets, no database, no clock: time is the tick counter. It loads
  content from disk once and is otherwise pure, which is what makes the
  simulation tests deterministic.
- **persistence** depends on game-core only for the `CharacterData` and
  `SaveRequest` types. It stores and loads aggregates; it never edits game
  state on its own (the only writes it originates are moderation actions).
- **server** wires the three together and is the only crate with Axum.
- **terminal-client** and **apps/web** depend on the protocol only. They
  render, predict local movement and send intent; they never decide damage,
  loot, positions or progression.

## The world actor

All gameplay mutation happens on one Tokio task. Inputs from connections
arrive on a bounded channel and are applied in arrival order between ticks.
A tick:

1. advances every player (regeneration, state machine, movement, strikes),
2. runs enemy AI and boss logic, then projectiles and hazards,
3. advances the boss encounter state machine,
4. collects pickups, reclaims remnants, detects zone changes, schedules
   interval saves,
5. routes the tick's output events to the players who should see them and
   sends sheets that changed,
6. every second tick, builds and queues one snapshot per player.

Because only this task touches the world, loot arbitration, kill rewards,
remnant drop and reclaim, and quest effects are trivially atomic: there is no
second writer to race with.

Snapshots are serialized once per recipient and offered to a bounded queue
with `try_send`. A full queue drops that snapshot (the next one supersedes it)
and resets the entity diff so despawns are not lost; 45 consecutive drops
disconnect the client. Reliable messages (events, sheets, chat, dialogue)
that cannot be queued disconnect the client immediately, because silently
losing them would desynchronise its view.

## Simulation model

- Positions are integer milli-cells (`1000` = one map cell). Collision is an
  axis-separated box test against walkable tiles; content validation keeps
  every per-tick displacement under half a cell, so walls cannot be skipped.
  Diagonal movement is scaled by 1/√2.
- Combat state machine per actor: `idle`, `moving`, `windup`, `active`,
  `recovery`, `rolling`, `parrying`, `guarding`, `staggered`, `healing`,
  `casting`, `interacting`, `dead`. All durations are ticks from content.
  Actions start only from free states; presses during busy states are
  buffered for 8 ticks.
- Enemy attacks lock their facing and shape when the windup starts; the
  telegraph cells sent to clients are exactly the cells that will be tested.
- Randomness (AI attack choice, drops, herald blinks, hazard placement) comes
  from a seeded SplitMix64 owned by the world.

## Persistence model

The character is an **aggregate** (`CharacterData`): attributes, echoes,
current health and draughts, position, checkpoint, lit lanterns, flags,
inventory, equipment and the dropped remnant. Saves write the whole
aggregate in one transaction, guarded by `version` (`update … where version <
$new`), together with the ledger rows describing what changed. Saves happen
on rest, death, respawn, pickup, door opening, dialogue effects, level-up,
boss reward, logout, shutdown and every 30 seconds.

Consequences, stated precisely:

- A crash loses at most the last 30 seconds of movement and any kill rewards
  earned since the last save. It can never duplicate currency or items,
  because there is no additive write: the stored state is always one
  coherent snapshot.
- Echoes in the remnant and echoes on the character are columns of the same
  row and change in the same statement.
- Loads go through the same FIFO as saves, so a character that reconnects
  quickly is read back only after its logout save committed.
- Out-of-order or replayed saves are rejected by the version check.

Enemies, ground loot, parties and the boss encounter live only in memory and
reset on restart by design (see GAME_DESIGN.md, "World rules").

## Interest management

Each snapshot contains entities within 34 cells (Chebyshev) of the recipient,
the player's own remnant, map pickups the character has not taken, its own
loot drops, and telegraphs in range. The server tracks which entity ids each
connection has seen and sends `gone` lists for those that left. Events are
delivered to players within the same radius of where they happened, or only to
the affected player for personal events (echoes, items, rests).

Clients additionally compute a field of view by raycasting from the player and
hide non-party entities outside it. That is presentation only; the server does
not rely on it.

## Concurrency and ownership rules

| state | owner | how others reach it |
|---|---|---|
| world, players, enemies, parties | world actor | `ToWorld` messages |
| socket I/O, framing, rate limits | connection task | `Outbound` queue |
| database writes of characters | persistence actor | `PersistCmd` queue |
| accounts, sessions, moderation | HTTP handlers / admin CLI | direct SQL |

One controlling connection per character and per account: a new login moves
the existing body to the new connection (or replaces the other character) and
the old socket receives `kicked` before it closes.

## Scaling path

v0 runs one world on one process and is sized for a small community; actual
capacity has to come from load tests on the target machine (see TEST_PLAN.md
for what was measured here). The boundaries already line up with the next
steps:

1. **Zones per worker.** `World` has no global state; a zone server would run
   one `World` per zone with the same actor loop. Entity ids would gain a zone
   prefix, and a zone transfer becomes "save aggregate, load on the other
   zone", which the versioned save already makes safe.
2. **Instanced arenas.** The boss encounter is a self-contained state
   machine keyed by the boss entity. A party instance would be a small
   `World` holding only the arena, created on demand.
3. **Gateway split.** Connection tasks only exchange messages with the world
   actor, so they can move to a gateway process that routes by zone, with
   Redis or NATS introduced only at that point.

No horizontal scaling is claimed for v0.

## Renaming

The working title appears in: crate names (`hollowrite-*`), the binary names
(`hollowrite`, `hollowrite-server`, `hollowrite-bot`), the cookie name
`hw_session`, the CSRF header `x-hollowrite`, environment variables
(`HOLLOWRITE_*`), the web `<title>` and title art, and the README. Content
files carry no branding beyond the tagline.
