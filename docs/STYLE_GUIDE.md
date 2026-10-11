# Presentation style guide

Both clients draw the same semantic world from `content/palettes/theme.json`.
The rules below are what that file encodes and what both renderers implement
(`crates/terminal-client/src/view.rs`, `apps/web/src/game/render.ts`).

## Mood

Bleak, quiet and readable. Most of the screen is charcoal and slate; warmth
comes only from lanterns, embers and echoes; danger is dried blood turning
bright as it approaches. Nothing flashes unless something is about to hurt
you.

## Palette

| name | hex | used for |
|---|---|---|
| void | `#0b0a09` | unexplored space, panel background |
| charcoal | `#1b1814` | floor ground |
| soot | `#25211b` | wall mass, roads, door frames |
| slate | `#4a5059` | dim floor detail, borders |
| slate_hi | `#78808c` | wall strokes, own-player backing |
| stone | `#6f6a60` | roads, rubble, blocked damage |
| rust | `#9a4a24` | doors, pews, shortcut iron |
| blood | `#6e1f1f` | telegraph ground, health bar back |
| blood_hi | `#b23a33` | health, knights, damage to you |
| amber | `#e8a644` | lanterns, embers, flask count, windup glyphs |
| amber_hi | `#ffd27a` | lantern flicker, interaction hints, criticals |
| gold | `#bf9a45` | names, titles, boss |
| fog | `#7f95ab` | wick, system chat |
| fog_hi | `#b9cbe0` | fog gate, zone titles, dodges |
| bone | `#ddd3bd` | text, bones, graves, shockwaves |
| ash | `#8c857a` | ash ground, muted text, pilgrims |
| sickly | `#93a34c` | stamina |
| sickly_hi | `#c4d36a` | remnant, party, hazards, phase-two boss |
| moss | `#4f5a34` | grass, stamina back, hazard ground |
| water | `#2f4a5e` | water, wick back |
| violet | `#8a76b8` | heralds |

In 16-colour mode each name maps to an ANSI colour (`ansi16` in the theme);
lighting collapses to "lit" and "dark grey". In 256-colour mode colours are
quantised to the xterm cube or grey ramp.

## Terrain glyphs

| tile | Unicode | ASCII | colour |
|---|---|---|---|
| wall | heavy box drawing joined to neighbouring edge walls (`┃ ━ ┏ ┓ ┗ ┛ ┣ ┫ ┳ ┻ ╋ ■`), blank inside thick masses; the browser strokes the same joins as lines | `#` | slate_hi on soot |
| floor | `·` or blank, chosen by a per-cell hash | `.` | slate on charcoal |
| road | `·` `∙` or blank | `.` | stone on soot |
| ash | `,` `'` `` ` `` | `,` | ash |
| grass | `"` `'` `,` | `"` | moss |
| tree | `♣` `♠` | `T` | ash |
| water | `≈` `~` (gently animated) | `~` | fog on water |
| bones | `∴` `∵` | `;` | bone |
| rubble | `▪` `▫` | `%` | stone |
| door, closed / open | `+` / `'` | same | rust |
| shortcut, closed / open | `╪` / `'` | `+` / `'` | rust |
| fog gate | `░` `▒` shimmering, brighter when sealed | `&` | fog_hi on slate |
| lantern | `☼` flickering | `*` | amber |
| ember pit | `░` `▒` pulsing | `^` | amber on blood |
| grave, pew, altar, fence, stairs | `†` `═` `╤` `│` `≡` | `t` `=` `_` `\|` `>` | bone, rust, gold, stone, stone |

The per-cell hash is identical in both clients (`cell_hash`), so the same
floor looks the same everywhere.

## Entities

| entity | Unicode | ASCII | distinguishing cues besides colour |
|---|---|---|---|
| you | your sigil (`@ & Ѻ Ж`) | `@ &` | lit backing cell, never hidden by fog of war |
| party member | sigil | sigil | underline, `+` after the name label, always drawn |
| stranger | sigil | sigil | plain name label |
| Hesper / Ansel / Moth | `Ħ` `Å` `ɱ` | `H A M` | gold name label |
| Oathless Knight | `K` | `K` | capital letter, bold |
| Ashen Pilgrim | `p` | `p` | lowercase (weaker), keeps distance |
| Starveil Herald | `Ψ` | `h` | unique silhouette |
| Bell-Mother | 3×2 sprite `▄█▄ / █Ω█`, cracked `▄▓▄ / █Ω▓` | `/^\ / \|B\|` | size; the sprite changes in phase two |
| projectile | `•` censer, `✶` ember | `o` `*` | |
| loot / key item | `♦` / `◆` pulsing | `$` / `k` | |
| your remnant | `◊` pulsing | `&` | HUD gives distance and direction |
| dead | `%` | `%` | dimmed |

Player sigils and colours come from fixed tables of six readable hues; a
player cannot pick a colour that disappears into the background.

## Combat readability

Every enemy attack is announced three ways that do not depend on colour:

1. **The attacker**: an `!` appears above it and its glyph turns amber for
   the whole windup (a `?` while it is staggered).
2. **The ground**: the exact cells the attack will test are overlaid with a
   glyph that names the attack type, and their background brightens as the
   hit approaches; in the last quarter of the windup they turn bold.

   | kind | Unicode | ASCII | meaning |
   |---|---|---|---|
   | strike | `×` | `x` | ordinary melee or projectile path |
   | heavy | `▓` | `X` | heavy hit; guarding costs double stamina |
   | area | `░` | `o` | circle on the ground |
   | shockwave | `◌` | `O` | expanding ring; cannot be blocked |
   | hazard | `≈` | `~` | lingering pool |
   | weak | `+` | `+` | the boss is open; hit her now |
3. **The text**: the boss bar says "cracked" in phase two and
   "OPEN — STRIKE" during the vulnerability window; banners announce phase
   changes and deaths.

When a hit lands the target flashes inverted for 120 ms and a number rises
from it: plain for damage, `!` suffix for criticals, `dodge`, `PARRY`, or
the blocked amount in grey. Swings draw a three-cell arc in the swing
direction (`│ ─ ╱ ╲`, heavy `┃ ━`).

## Light and fog of war

- Field of view is a raycast of radius 18 from your cell; walls, trees,
  closed doors and fog block it.
- Visible cells are lit by your own light (radius 15, falloff to an ambient
  floor of 0.42) and by lanterns (radius 8, gentle flicker).
- Cells seen before but not visible now are drawn dark (0.22).
- Unexplored cells are empty.
- Enemies and strangers outside your view are not drawn; party members
  always are. Telegraph cells are drawn only where you can see.

## Layout

Terminal:

- **Minimum 80×24.** Smaller terminals get a message asking to enlarge.
- **80–109 columns:** two HUD lines (bars with numbers, zone, interaction
  hint, remnant direction, network indicator), the world, and a two-line
  footer (latest chat line and key hints). Character, party and quest panels
  open as overlays.
- **110×30 and larger (target 120×35):** a 34-column side panel (character,
  equipment, party, quests) and an 8-line chat panel.
- The camera keeps you centred; the map is drawn 1:1 in cells.
- Overlays (gear, levelling, dialogue, map, help, menu) are centred boxes;
  dialogue sits at the bottom so the speaker stays visible.

Browser:

- The canvas fills the space beside a 300 px side panel and above the chat;
  under 900 px wide the side panel hides.
- Cells are sized from the measured width of `M` at the chosen font size
  (11–26 px), and the canvas backing store follows `devicePixelRatio`, so
  glyphs stay crisp and aligned on every display.
- Login, characters, chat, menus and dialogs are ordinary accessible DOM.

## Fonts

Recommended: DejaVu Sans Mono, Cascadia Mono, JetBrains Mono, Iosevka,
Menlo, Consolas. All glyphs in the theme are single-width in `unicode-width`
(a test enforces it). Terminals in East Asian locales may render some of them
(box drawing, `♣`, `≈`, `∴`) double-width; use `--ascii` there.

## Motion and contrast options

- Reduced motion (terminal `--reduced-motion`, browser menu, and the
  browser respects `prefers-reduced-motion`): no lantern flicker, embers,
  water, pits or fog animation, no rising numbers.
- High contrast: lighting is boosted by 35%.
- ASCII glyphs: pure 7-bit ASCII for any font.
- Colour depth: 24-bit, 256 or 16 (terminal).

## Captures

`docs/captures/terminal-alice-sees-bob.png` and
`docs/captures/browser-bob-sees-alice.png` show both clients at the same
moment from the same server. The terminal capture is reproducible with
`scripts/capture-terminal.sh` (tmux plus `scripts/ansi2html.py`; the raw
ANSI capture is committed next to the PNG), and the browser one with
`scripts/browser-session.mjs` (Playwright).
