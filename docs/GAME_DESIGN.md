# Game design

## Premise

Long ago the kingdom of Veyr tried to bind mortality with a ceremonial bell
and a covenant with something buried beneath the stars. The ceremony, the
Hollow Rite, broke the boundary between death, memory and language. The dead
return, and each return unthreads part of who they were. Knights guard crowns
whose kings have no names. The Pale Synod harvests recollections to stretch
its own lives. The constellations are wrong.

The player is a **Hollowbound**: revived at a dying lantern with fragments of
a name. Lore is carried by places, item descriptions and three short-spoken
NPCs rather than exposition.

Factions are seeded as reputation flags only (`rep:last_lantern`,
`rep:pale_synod`; the Thorn Court is met in the secret grove). Faction play
is out of scope for v0.

## Pillars, and how the slice serves them

| pillar | in the slice |
|---|---|
| Every action costs something | Attacks, rolls and parries spend stamina; regeneration pauses 18 ticks after spending and stops while guarding. Rites spend wick and have cooldowns. Draughts are few and slow to drink. |
| Information is mastery | Every enemy attack has a windup of 12–34 ticks, shown as tinted cells with a type-specific glyph and an `!` over the attacker. Shapes are locked when the windup starts, so the telegraph is exact. |
| The world remembers | Character, gear, echoes, remnant, lanterns, doors, quest flags and position persist across disconnects and restarts. |
| Other players are real | One shared world; party members are marked and always drawn; the boss arena is shared and scales with participants. |
| Text is an art medium | A disciplined 24-bit palette, lighting and field of view, box-drawn walls, animated lanterns, embers, fog and pits, readable telegraphs, ASCII and 16-colour fallbacks. |
| Vertical slice first | One connected level, three enemy types, one boss, two quests, one secret. |

## First session

1. Register, create a character (name, origin, sigil, colour).
2. Wake beside **Hesper** at the Last Lantern. She sends you to light the
   lantern in Gloam Plaza.
3. At the crossroads choose the **Lampless Streets** (knights, pilgrims, a
   chapel) or the **Ash Orchard** (pilgrims, a knight, a herald, a hidden
   grove). Both reach the plaza; both are longer than they look.
4. Light the plaza lantern. Open the **collapsed aqueduct** from the plaza
   side: a safe, short route back to the crossroads for every later run.
5. Back at the Last Lantern, Hesper rewards you and points you to **Ansel**,
   who wants the Sexton's Key from the chapel in the streets.
6. Take the key past the knights, open the **Ossuary gate** in the plaza.
7. Choose the crypt (knights, a herald, the Vigil Hauberk and the Hollow Toll
   rite) or the bone pits (ember pits, pilgrims). Light the Sexton's Lantern
   and open the inner shortcut back to the Ossuary entrance.
8. Fight **the Bell-Mother**, alone or with a party. Die, retry from the
   Sexton's Lantern, reclaim your echoes.
9. Bring the **Clapper of Names** to Hesper (a larger flask) or to Ansel (more
   echoes). Only one of them gets it.

Explored normally, with a few deaths, this is roughly 30–60 minutes. That
estimate comes from the size of the map, enemy count and fight lengths
measured by the duel tests, not from external playtests; it needs real
players to confirm.

## Origins

Starting loadouts, not classes; every attribute can be raised later.

| origin | VIT | END | MIG | FIN | OCC | gear | trait |
|---|---|---|---|---|---|---|---|
| Ash Warden | 12 | 12 | 11 | 8 | 6 | Rustbrand Sword, Lantern Buckler | can parry from the start |
| Veil Seer | 8 | 9 | 6 | 9 | 15 | Thorn Knife, Ember Psalm, 2 Wax Seals | ranged rite, large wick pool |
| Grave Runner | 10 | 11 | 8 | 14 | 6 | Grave Sickle, 1 Grave Salt | quick roll (15 ticks, i-frames 1–10) |

## Attributes and formulas

All values live in `content/balance.toml` and `content/items/items.toml`.

- Health = 180 + 22 × Vitality
- Stamina = 60 + 4 × Endurance; regenerates 1 per tick (30/s) after an
  18-tick pause; no regeneration while guarding or mid-swing.
- Wick = 20 + 4 × Occult; regenerates 1 per second.
- Weapon damage = base + (Might × might% + Finesse × finesse%) / 100, then ×
  the attack's multiplier (light 100%, heavy 150–180%).
- Rite damage = base + Occult × occult% / 100.
- Mitigation: damage × 100 / (100 + 2 × defense), minimum 1.
- Poise: player poise is 20 + armour poise; enemies have their own. Reaching
  zero staggers. An enemy that recovers from a stagger ignores poise damage
  for 75 ticks, so a stagger is a punish window, not a lock.
- Riposte: a successful parry staggers the attacker for 45 ticks; the next
  hit within that window deals 250%.
- Level cost from level L: ⌊100 × 1.09^(L−1)⌋ + 15 × L echoes
  (115, 139, 163, 189, 216, 243, …). Attributes cap at 40. Levelling happens
  only beside a lantern the character has lit.

## Combat

Frame data is in ticks (30 per second).

| action | windup | active | recovery | stamina | notes |
|---|---|---|---|---|---|
| light (sword) | 9 | 3 | 11 | 20 | poise 18 |
| heavy (sword) | 21 | 4 | 17 | 34 | poise 45, ×1.65 |
| roll (standard) | — | — | 18 total | 24 | i-frames 2–9, 3.2 cells |
| roll (quick) | — | — | 15 total | 20 | i-frames 1–9, 3.4 cells |
| parry | — | 6-tick window | 22 total | 14 | shield only, parryable attacks only |
| guard | held | — | — | per hit | 75% block with buckler; stamina cost 60% of raw damage (×2 vs heavy) |
| draught | 30 | heals at 18 | — | — | 45% health; interrupted by a stagger before tick 18 keeps the draught |

Rules worth knowing:

- Attacks begin only from idle, moving or guarding. A press during another
  action is buffered for 8 ticks.
- Light and heavy attacks turn toward the nearest enemy in reach if none is
  already in the swing arc. They never extend reach.
- Hits need line of sight from the attacker's cell to the target's cell.
- Guarding only works against attacks from the front half.
- Stamina may run to zero but never below; an action needs at least 1.

## Enemies

| | Oathless Knight | Ashen Pilgrim | Starveil Herald |
|---|---|---|---|
| role | melee bruiser | ranged harasser | occult controller |
| glyph | `K` | `p` | `Ψ` (ASCII `h`) |
| health / defense / poise | 320 / 20 / 90 | 190 / 8 / 35 | 280 / 12 / 40 |
| attacks | cleave (120° arc, parryable), lunge (3-cell thrust, heavy) | censer (projectile with a line telegraph), shove (parryable) | starfall (circle on your position, 34-tick warning), nova (circle around itself) |
| behaviour | chases, attacks with a 0.6–1.5 s gap between attacks | keeps about 4.5 cells away, retreats for at most 2 s then holds ground | keeps distance, blinks away when you close in |
| echoes | 60 | 45 | 90 |

Enemies leash back home beyond 15–16 cells, healing fully; they cannot enter
the sanctuary. Dead enemies return after 90 seconds once no player is within
12 cells of their spawn point.

## The Bell-Mother

A bronze bell on a body, the clapper still inside. 2600 health, drawn as a
3×2 sprite.

**Phase one** (above 50%): measured sweeps (180° arc, parryable), a delayed
slam in front of her (heavy), and a toll around herself on cooldown.

**Transition** at 50%: she kneels for 2.5 seconds and cannot be damaged; the
bell cracks (sprite and colour change, banner "THE BELL CRACKS").

**Phase two**: faster, wider sweeps and slams, plus:

- **Peal** (every ~7 s): a 30-tick windup, then three expanding shockwave
  rings 0.8 s apart. A ring is a band about one cell wide moving outward one
  cell every 3 ticks; it cannot be blocked and must be rolled through or
  outrun.
- **Dirge pools**: every 10 s, three pools form near participants with a
  1.2 s warning and linger for 6 s, ticking for 18 every half second.
- **Open bell**: after each peal she hangs open for 3 seconds, takes 160%
  damage and does not attack (a `+` ring marks her and the HUD says
  "OPEN — STRIKE").

### Arena semantics (co-op)

- The arena is a single shared encounter, not an instance.
- The encounter starts when the first living player crosses into the belfry.
  For the next 5 seconds anyone else who enters joins; the boss's health is
  rescaled (×(1 + 0.7 × (n − 1))) and poise (×(1 + 0.3 × (n − 1))) with
  damage already dealt preserved.
- After 5 seconds the fog gate seals: nobody enters or leaves. Outsiders
  cannot interfere and cannot be pulled in.
- If every participant dies or disconnects, the encounter resets after 3 s:
  full health, phase one, hazards cleared, gate open.
- On defeat, every participant still alive and connected receives the
  reward (2000 echoes and the Clapper on a first kill, 25% of the echoes on
  repeat kills). A participant who died has already respawned at a lantern
  and is no longer part of the fight, which keeps "die early, collect later"
  from paying. The boss returns
  90 s later once the arena is empty.
- Path to party instances later: the encounter state machine is keyed by the
  boss entity and only reads players inside the arena rectangle, so it can be
  hosted in a per-party instance `World` without changing the rules.

## Death

On death, all unspent echoes become a **remnant** (◊) where you fell, or at
your last safe position if you died on an ember pit. After 2.5 s you wake at
your checkpoint lantern with full health, stamina, wick and draughts.

Touching the remnant returns its echoes. Dying again before that destroys the
old remnant for good and leaves a new one with whatever you carried. This is
the deliberate permanent-loss rule: you can lose at most one life's worth of
echoes, and levels, items and gear are never lost.

Remnant drop and reclaim happen on the world actor and are saved immediately,
in the same row as the character's echoes, so they cannot be duplicated by
dying, reclaiming and disconnecting in any order.

## World rules

| state | persisted | resets |
|---|---|---|
| character aggregate (stats, echoes, remnant, gear, flags, lanterns, position) | yes | never |
| doors and shortcuts opened | yes, per character | never |
| map pickups taken | yes, per character | never; each character finds each pickup once |
| enemies and their health | no | on death respawn timer, on leash, on server restart |
| enemy drops on the ground | no (owner-only, 3 minutes) | on logout or timeout |
| boss encounter | no | when all participants are gone, or 90 s after defeat |
| parties, invites | no | on logout or restart |

Resting at a lantern restores the resting character only. It does not reset
enemies, because in a shared world that would let one player reset everyone
else's fights.

## Exclusions from v0

Faction reputation effects, trading, an economy beyond echoes, crafting,
PvP, invasions, more than one region, party instances, guilds, mail,
password reset, email verification, mobile and touch play, controller
support, sound. Each is a reasonable next step; none is hinted at in the UI.
