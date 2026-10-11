# Content pipeline

All game content is plain text in `content/`, loaded once at server start
by `Content::load` in `crates/game-core/src/content.rs` and validated before
the server accepts players. `hollowrite-server check-content` runs the same
validation without starting anything.

```
content/
  balance.toml            global tuning: tick rate, movement, stamina, rolls, parry, levels, boss scaling
  items/items.toml        weapons, shield, rites, armour, consumables, keys
  actors/archetypes.toml  starting origins
  actors/enemies.toml     enemy templates, attacks, drops, boss phase two
  dialogue/npcs.toml      NPC dialogue nodes, conditions and effects
  dialogue/quests.toml    quest stages derived from flags
  maps/veyr.map           the grid
  maps/veyr.toml          legend, markers and zones for the grid
  palettes/theme.json     glyphs and colours shared by both clients
```

## Units

- Durations are **ticks** (30 per second unless `tick_hz` changes).
- Distances, reaches, radii and speeds are **milli-cells**: `1000` is one map
  cell; speeds are milli-cells per tick.
- Percentages are integers (`mult_pct = 165` is ×1.65).

## Maps

`veyr.map` is a rectangle of single characters, one row per line (lines
starting with `;` are ignored). Plain terrain uses the tile codes:

| code | tile | walk | blocks sight |
|---|---|---|---|
| ` ` | void | | ✓ |
| `#` | wall | | ✓ |
| `.` | floor | ✓ | |
| `:` | road | ✓ | |
| `,` | ash | ✓ | |
| `"` | grass | ✓ | |
| `T` | tree | | ✓ |
| `~` | water | | |
| `b` | bones | ✓ | |
| `%` | rubble | ✓ | |
| `+` | door (open doorway unless the legend gates it) | ✓ | |
| `\|` | fence | | |
| `>` | stairs | ✓ | |
| `^` | ember pit (14 damage every half second) | ✓ | |
| `A` | altar | | |
| `t` | grave | ✓ | |
| `=` | pew | | |

Every other character must be defined in the `[legend]` of `veyr.toml`. A
legend entry names the base tile and what the marker adds:

```toml
"k" = { tile = ".", enemy = "oathless_knight" }
"2" = { tile = "L", lantern = "gloam_lantern", name = "Gloam Plaza Lantern" }
"g" = { tile = "+", door = "ossuary_gate", rule = { kind = "key", key = "sexton_key" } }
"a" = { tile = "D", door = "tunnel_shortcut", rule = { kind = "oneway", from = "e" } }
"x" = { tile = "X", door = "grove_wall", rule = { kind = "secret" } }
"f" = { tile = "G", door = "belfry_gate", rule = { kind = "fog" } }
"w" = { tile = ".", item = "wax_seal", qty = 2 }
```

Special tiles (`L` lantern, `D` shortcut, `G` fog gate, `X` false wall) can
only be placed through markers, so each one has an id. A door id may cover
several tiles (the Ossuary gate is two cells wide). Map pickups get the id
`<item>@<x>,<y>` and each character can take each one once.

Door rules: `key` opens for a character holding the key; `oneway` opens
when interacted with from the side named by `from`; `secret` opens on
interaction and is drawn as a wall until then; `fog` is passable unless the
boss encounter has sealed it. Opened doors are per character.

Zones are rectangles; later zones override earlier ones where they overlap.
`safe = true` keeps enemies out. Exactly one zone should be `arena = true`.

## Validation performed at load

- Grid is rectangular, every glyph is a tile code or a legend marker, the
  start lantern exists, zones fit the map.
- Every referenced item, enemy and NPC exists; doors sit on door tiles; key
  doors name a real key.
- Movement, rolls, projectiles and enemies never move half a cell or more per
  tick (prevents tunnelling through one-cell walls).
- Roll invulnerability frames fall within the roll; parry window within the
  parry; the draught heals within its windup; every attack has windup and
  active frames.
- Every item of each kind has the matching data block.

Additional content tests (`cargo test -p hollowrite-game-core --test content`):

- every lantern, spawn and pickup is reachable from the start with all doors
  open; the boss and the grove are **not** reachable without the key and the
  secret; the tunnel shortcut cuts the trip by at least a quarter and has no
  enemies;
- the map border is closed and no enemy spawns in the sanctuary;
- any dialogue node or choice that grants items, echoes or draughts forbids a
  flag it sets (no farming).

The `playthrough` tests then walk a character through the whole quest chain,
the secret and both shortcuts using only inputs.

## Dialogue and quests

NPC dialogue is an ordered list of nodes. The first node whose `requires`
flags are all set, `forbids` flags all unset and `has_item` items all held
is spoken. Node `effects` apply when it is spoken; choice `effects` apply
when chosen. Effects: `set` flags, `give`/`take` items, `echoes`,
`draughts_max`. Takes are checked before anything is granted, so a choice
either applies completely or not at all.

Flags the engine sets on its own: `lit:<lantern>`, `door:<door>`,
`took:<pickup>`, `got:<key item>`, `slain:<boss>`.

A quest appears once its `start` flag is set, is complete when its `done`
flag is set, and shows the note of the last stage whose flags all hold.

## Theme

`palettes/theme.json` maps tile codes, entities and telegraph kinds to
glyphs (Unicode and ASCII) and palette names, defines the 24-bit palette and
its 16-colour equivalents, and sets lighting radii. The terminal client
embeds it at compile time; the web client imports it at build time. See
STYLE_GUIDE.md.

## Changing content safely

1. Edit the TOML/JSON/map file.
2. `cargo run -p hollowrite-server -- check-content`
3. `cargo test -p hollowrite-game-core` (content, playthrough, balance)
4. Restart the server. Content is not hot-reloaded.

Renaming ids that characters already reference (items in inventories,
flags, lantern ids) needs a data migration; prefer adding new ids.
