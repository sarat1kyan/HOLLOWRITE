# Content manifest (v0)

Authoritative values are in `content/`; this page summarises them. The
`content` and `balance` test suites fail if the files stop matching the rules
described here (reachability, once-only rewards, frame-data sanity).

## Level: Veyr (128 × 52 cells)

| zone id | name | safe | contents |
|---|---|---|---|
| `last_lantern` | The Last Lantern | yes | spawn lantern, Hesper, Ansel (in the chapel ruin) |
| `crossroads` | Gloamreach Crossroads | | the fork; west side of the tunnel shortcut |
| `tunnel` | Collapsed Aqueduct | | one-way shortcut, opened from the plaza side |
| `streets` | The Lampless Streets | | 4 knights, 2 pilgrims, Sexton's chapel, Sexton's Key, 2 Ashen Moths, Wax Seals |
| `orchard` | Ash Orchard | | 2 knights, 2 pilgrims, 1 herald, Grave Salt, Ashen Moth, the false wall |
| `grove` | Thornhollow Grove | | secret: Moth, Penitent Maul |
| `plaza` | Gloam Plaza | | second lantern; Ossuary gate (Sexton's Key) |
| `ossuary` | The Ossuary | | 3 knights, 2 pilgrims, 1 herald, ember pits, Vigil Hauberk, Hollow Toll, Wax Seals, Grave Salt, Sexton's Lantern, inner one-way shortcut |
| `belfry` | Belfry of Names | | the Bell-Mother arena behind a fog gate |

Totals: 9 Oathless Knights, 6 Ashen Pilgrims, 2 Starveil Heralds, 1 boss,
3 lanterns, 5 gated doors (key gate, two one-way shortcuts, false wall, fog
gate), 11 map pickups.

## Enemies

| id | level | hp | def | poise | speed (cells/s) | sight | leash | echoes | drops |
|---|---|---|---|---|---|---|---|---|---|
| `oathless_knight` | 4 | 320 | 20 | 90 | 2.55 | 8 | 16 | 60 | Grave Salt 20%, Rustbrand Sword 6% |
| `ashen_pilgrim` | 3 | 190 | 8 | 35 | 2.25 | 9 | 15 | 45 | Wax Seal 25%, Ashen Moth 10% |
| `starveil_herald` | 6 | 280 | 12 | 40 | 2.10 | 10 | 15 | 90 | Wax Seal 30%, Thorn Knife 5% |
| `bell_mother` | 12 | 2600 (co-op scaled) | 25 | 160 | 1.8 → 2.25 | arena | arena | 2000 | Clapper of Names (first kill) |

Attacks (damage / windup ticks / shape):

- Knight: cleave 72 / 16 / 120° arc 1.9 cells, parryable; lunge 95 / 22 /
  3.2-cell line, heavy, 2 s cooldown.
- Pilgrim: censer 52 / 20 / projectile 9 cells/s, 8-cell range; shove 30 /
  12 / 140° arc, parryable.
- Herald: starfall 72 / 34 / 1.6-cell circle on the target; nova 45 / 15 /
  2.2-cell circle around itself; blink 4 cells, 3 s cooldown.
- Bell-Mother phase 1: sweep 95 / 24 / 180° arc 3.6 cells (parryable); slam
  130 / 30 / 2-cell circle 2.2 ahead (heavy); toll 80 / 26 / 2.6-cell circle.
- Bell-Mother phase 2: sweep 105 / 18 / 200°; slam 140 / 26 / 2.2-cell
  circle; peal: three rings of 70 each; dirge pools 18 per half second.

Drops are rolled once per contributor per kill and appear only for that
player, so there is no loot competition between strangers.

## Weapons, shield, armour, rites

| id | name | damage | scaling MIG/FIN % | reach | arc | light (w/a/r, stamina) | heavy (w/a/r, stamina, ×) |
|---|---|---|---|---|---|---|---|
| `rustbrand_sword` | Rustbrand Sword | 34 | 150 / 100 | 1.7 | 110° | 9/3/11, 20 | 21/4/17, 34, ×1.65 |
| `thorn_knife` | Thorn Knife | 22 | 50 / 180 | 1.3 | 70° | 6/2/8, 13 | 15/3/13, 24, ×1.5 |
| `grave_sickle` | Grave Sickle | 28 | 70 / 170 | 1.6 | 140° | 8/3/9, 16 | 18/4/14, 28, ×1.6 |
| `penitent_maul` | Penitent Maul | 52 | 240 / 20 | 1.8 | 120° | 15/4/17, 28 | 30/5/24, 44, ×1.8 |

- `lantern_buckler` Lantern Buckler: blocks 75%, guard stamina 60% of raw
  damage, can parry.
- `ember_psalm` Ember Psalm: bolt, 12 wick, 0.8 s cooldown, 40 + 3 × Occult,
  9-cell range.
- `hollow_toll` Hollow Toll: area around the caster, 26 wick, 5 s cooldown,
  55 + 3.5 × Occult, 2.6 cells, needs Occult 10.
- `mourners_wraps` Mourner's Wraps: defense 6, poise 10 (all origins start
  with it).
- `vigil_hauberk` Vigil Hauberk: defense 18, poise 30 (Ossuary crypt).

## Consumables and keys

| id | effect | stack | source |
|---|---|---|---|
| `ashen_moth` | +150 echoes | 20 | streets ×2, orchard, pilgrim drops |
| `wax_seal` | +25 wick | 10 | streets, Ossuary, Veil Seer start, drops |
| `grave_salt` | +120 health, instant | 10 | orchard, Ossuary, Hesper, Grave Runner start, knight drops |
| tallow draught | 45% health over a 30-tick drink | 3 base | refilled at lanterns; +1 from Moth, +1 from Hesper's ending |
| `sexton_key` | opens the Ossuary gate | — | Sexton's chapel |
| `tallow_cup` | proof of Moth's gift | — | Moth |
| `clapper_of_names` | quest item for the ending choice | — | Bell-Mother |

## NPCs and quests

- **Hesper**, Keeper of the Last Lantern (Last Lantern): tutorial, first
  quest, one of the two endings.
- **Ansel the Unwritten**, Pale Synod (chapel ruin in the sanctuary): second
  quest, the other ending.
- **Moth**, Thorn Court (secret grove): extra draught.

Quest *A Second Light*: light the Gloam Plaza lantern and report back
(reward: 2 Grave Salt).

Quest *The Tolling Below*: fetch the Sexton's Key, open the Ossuary, defeat
the Bell-Mother, give the Clapper to Hesper (+1 draught, 600 echoes,
`rep:last_lantern`) or Ansel (1500 echoes, `rep:pale_synod`).

Every dialogue node that grants something forbids a flag it sets, so rewards
cannot be farmed by talking repeatedly; a content test enforces this.

## Economy assumptions

- One clear of every regular enemy pays 990 echoes; the three Ashen Moths add
  450; the boss pays 2000 on a first kill.
- Levels 1→7 cost 1065 echoes in total, so a first session that clears most
  of the level before the boss reaches roughly level 6–8, and the boss reward
  is worth several more levels.
- Death costs at most the echoes carried at the time. Levels and items are
  never lost.

## Measured fight lengths

From `crates/game-core/tests/balance.rs` (scripted fighter that dodges what
is telegraphed at it and strikes when the enemy is not winding up; six seeds
each, level 1):

| matchup | wins | average health left | average duration |
|---|---|---|---|
| Ash Warden vs knight | 6/6 | 82% | 5.3 s |
| Grave Runner vs knight | 6/6 | 80% | 4.2 s |
| Ash Warden vs pilgrim | 6/6 | 96% | 15.3 s |
| Grave Runner vs pilgrim | 6/6 | 88% | 17.9 s |
| Ash Warden vs herald | 6/6 | 82% | 29.6 s |
| Grave Runner vs herald | 6/6 | 81% | 20.3 s |
| idle Ash Warden vs knight | dies | — | 14.9 s |

The scripted fighter reacts perfectly to telegraphs, so these numbers are
easier than human play; the danger in the level comes from groups and from
mistakes. The boss has not been balanced by playtesting yet.
