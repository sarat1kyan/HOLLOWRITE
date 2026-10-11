# Network protocol (version 1)

Source of truth: `crates/protocol/src/lib.rs`. TypeScript mirror:
`apps/web/src/protocol.ts`. Both are checked against
`apps/web/src/__fixtures__/protocol.json`, which a Rust test regenerates
(`HOLLOWRITE_WRITE_FIXTURES=1 cargo test -p hollowrite-protocol`) and
otherwise verifies.

## Transport and authentication

1. **HTTP auth** (JSON bodies, all `POST` need header `x-hollowrite: 1`):
   - `POST /api/register {username, password, client}`
   - `POST /api/login {username, password, client}`
   - `POST /api/logout`
   - `GET /api/me`

   `client: "web"` gets an `HttpOnly; SameSite=Strict` cookie `hw_session`
   and no token in the body. `client: "terminal"` or `"bot"` gets
   `{token}` in the body and no cookie.
2. **WebSocket** `GET /ws`:
   - With an `Origin` header (every browser): the origin must be in the
     allow-list and the cookie authenticates. A bearer header is ignored.
   - Without `Origin` (native clients): `Authorization: Bearer <token>` is
     required and cookies are ignored.
   - Tokens never travel in URLs. Unauthenticated upgrades get 401, bad
     origins 403, too many connection attempts from one IP 429.
3. Frames are UTF-8 text, one JSON object each, at most 4 KiB from the
   client. Larger frames close the connection.

## Session flow

```
client                                    server
  hello {protocol, client, build} ───────►
                                  ◄─────── welcome {protocol, server, tick_hz,
                                                    snapshot_hz, account,
                                                    walk_speed, body_radius}
  list_characters ───────────────────────►
                                  ◄─────── characters {list}
  create_character {name, archetype, look} ►
                                  ◄─────── characters {list} | error
  select_character {id} ─────────────────►
                                  ◄─────── enter_world {you, tick, map, sheet}
                                  ◄─────── party {...}
  input {seq, mv?, act?, face?} ─────────►  (any rate up to the limit)
                                  ◄─────── snapshot (15 Hz)
                                  ◄─────── events / sheet / chat / dialogue
  leave ─────────────────────────────────►
                                  ◄─────── characters {list}
```

A `hello` with a different protocol version receives
`error {code: "protocol_mismatch"}` and the socket closes. Any message
other than `hello` first is an error and closes the socket.

## Client messages

| `t` | fields | notes |
|---|---|---|
| `hello` | `protocol`, `client` (`web`/`terminal`/`bot`), `build` | first frame |
| `list_characters` | | lobby only |
| `create_character` | `name`, `archetype`, `look {sigil 0..3, hue 0..5}` | names: 3–16 ASCII letters/digits/`-`/`_`, leading letter, unique ignoring case |
| `select_character` | `id` | must belong to the account |
| `input` | `seq` (strictly increasing), `mv?` (held direction), `act?`, `face?` | stale or duplicate `seq` is dropped |
| `equip` | `slot` (`main`/`off`/`armor`/`spell1`/`spell2`), `item` or `null` | validated against inventory, kind and requirements |
| `use_item` | `item` | consumables only, while free |
| `level_up` | `attr` | beside a lit lantern, enough echoes, below cap |
| `talk` | `npc`, `choice?` | in range; a choice must belong to the node last shown |
| `chat` | `channel` (`local`/`party`), `text` | sanitised, ≤200 chars, 5 burst then 1/s |
| `party_invite` / `party_accept` / `party_leave` | `name` / `from` / — | max party 4, max 8 pending invites |
| `mute` | `name`, `on` | per account, persisted |
| `report` | `name`, `reason` | stored for moderators |
| `ping` | `nonce` | answered with `pong` |
| `resync` | | resends `enter_world` and party state |
| `leave` | | back to the lobby; the character is saved |

`act` values: `light`, `heavy`, `roll`, `parry`, `guard_on`, `guard_off`,
`heal`, `cast1`, `cast2`, `interact`.

Movement is held state: the latest `mv` applies every tick until replaced,
and expires on the server after 12 ticks without a newer input. Clients
re-send held movement every 100 ms. The server never accepts positions,
damage, item quantities or targets from clients.

## Server messages

| `t` | contents |
|---|---|
| `welcome` | protocol, server build, tick and snapshot rates, account name, movement constants for prediction |
| `error` | `code`, human-readable `message` |
| `characters` | summaries for the lobby |
| `enter_world` | your entity id, tick, the map (rows of tile codes, zones, lanterns, gated doors), your sheet |
| `snapshot` | `tick`, `ack` (highest input `seq` applied), `me` (private stats, boss bar, interaction hint, remnant), `entities` in range, `gone` ids, `tele` telegraphs |
| `sheet` | character sheet after any change (inventory, equipment, quests, open doors, derived stats) |
| `events` | one batch per tick: hits, swings, deaths, rewards, lantern, boss phase, notices, zone changes |
| `chat` | `channel`, `from`, `text` (`system` channel for server notices) |
| `party` | members with health, pending invites |
| `dialogue` | NPC id, speaker, lines, choices |
| `pong` | nonce, tick |
| `kicked` | reason, sent before the server closes a replaced or suspended connection |

### Entities

`EntityView {id, kind, def, name?, x, y, facing, state, action?, progress,
rel, hp?, look?, phase?}`. Positions are milli-cells. `rel` is computed per
recipient: `me`, `party`, `stranger`, `neutral` (NPCs), `hostile`.
`progress` is the percentage through the current state, enough for clients
to animate windups.

### Telegraphs

`Telegraph {id, owner, kind, cells: [[x, y]], left, total}` with `kind` in
`strike`, `heavy`, `area`, `shockwave`, `hazard`, `weak`. `cells` are the
exact cells the server will test when the attack resolves. `left` is ticks
until it lands; clients count down locally between snapshots.

## Ordering, loss and reconnection

- Snapshots carry the server tick; a client applies them in arrival order
  (TCP preserves order) and interpolates remote entities 110 ms in the past.
- `ack` lets the client reconcile local movement prediction: it compares the
  authoritative position with where it predicted itself one round trip ago
  and eases the difference out (snapping only beyond 1.8 cells).
- A snapshot dropped for a slow client is superseded by the next; the server
  then resends the full entity set so nothing is left stale.
- After a reconnect the client simply logs in again and selects the
  character; the server restores it from memory if it never left, or from the
  database after its last save committed.

## Limits

| limit | value |
|---|---|
| client frame size | 4 KiB |
| client frame rate | 90 burst, 60/s sustained; 50 violations close the socket |
| hello timeout / idle timeout | 10 s / 30 s |
| server ping | every 10 s |
| outbound queue per connection | 512 frames; reliable overflow closes, 45 dropped snapshots close |
| world inbound queue | 4096 messages; inputs are dropped (not queued) under overload |
| registrations | `HOLLOWRITE_REGISTRATIONS_PER_HOUR` per IP |
| logins | 20/min per IP, 8/min per name, account lock for 5 min after 5 failures |
| WebSocket connects | 20 burst, 1/s per IP |
