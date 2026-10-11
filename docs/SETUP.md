# Setup and testing guide

This walks you from a clean machine to two players fighting in the same
world, one in a terminal and one in a browser. Every command here was run on
a fresh clone of `main` (Ubuntu 24.04, 4 cores) before this guide was
written; timings are from that run.

- [1. Install the tools](#1-install-the-tools)
- [2. Get the code and configure it](#2-get-the-code-and-configure-it)
- [3. Start PostgreSQL](#3-start-postgresql)
- [4. Build, migrate, test](#4-build-migrate-test)
- [5. Run the game](#5-run-the-game)
- [6. Play-test checklist (two players)](#6-play-test-checklist-two-players)
- [7. Automated checks against a running server](#7-automated-checks-against-a-running-server)
- [8. Playing on your LAN](#8-playing-on-your-lan)
- [9. Resetting](#9-resetting)
- [10. Troubleshooting](#10-troubleshooting)

---

## 1. Install the tools

| tool | version | why |
|---|---|---|
| Rust | stable, 1.87 or newer | server, terminal client, tests |
| Node.js | 20 or 22 (npm comes with it) | browser client |
| PostgreSQL | 16 (Docker or native) | accounts and characters |
| make | any | convenience targets (optional; every target is a one-liner) |
| tmux, Playwright | optional | only for scripted captures |

### Linux (Ubuntu / Debian)

```bash
sudo apt update
sudo apt install -y build-essential curl git make postgresql-16 postgresql-client-16
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
# Node 22 via NodeSource (or use nvm / your distro's nodejs 20+)
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt install -y nodejs
```

If you prefer Docker for the database, install Docker Engine and skip
`postgresql-16`.

### macOS (Homebrew)

```bash
brew install rustup node@22 postgresql@16
rustup-init -y && source "$HOME/.cargo/env"
brew services start postgresql@16
export PATH="$(brew --prefix postgresql@16)/bin:$PATH"   # add to your shell profile
```

### Windows

Use **Windows Terminal** (PowerShell) for the terminal client; the classic
console works but renders worse.

1. Install Rust from <https://rustup.rs> (accept the Visual Studio Build
   Tools prompt).
2. Install Node.js 22 LTS from <https://nodejs.org>.
3. Install PostgreSQL 16 from <https://www.postgresql.org/download/windows/>
   *or* Docker Desktop.
4. `make` is not needed: sections 4 and 5 list the plain commands. Set
   environment variables in PowerShell like this before running them:

   ```powershell
   $env:DATABASE_URL = "postgres://hollowrite:hollowrite@127.0.0.1:5432/hollowrite"
   $env:TEST_DATABASE_URL = "postgres://hollowrite:hollowrite@127.0.0.1:5432/hollowrite_test"
   ```

Alternatively run everything inside WSL 2 (Ubuntu) and follow the Linux
steps; the browser on Windows reaches `http://127.0.0.1:5173` in WSL.

Check what you have:

```bash
make check-env
```

```
toolchain
  ok    cargo      cargo 1.97.0
  ok    node       v22.22.0
  ...
database
  --    not reachable; run 'make db-up'
ports
  free  7700
  free  5173
```

## 2. Get the code and configure it

```bash
git clone https://github.com/sarat1kyan/HOLLOWRITE.git
cd HOLLOWRITE
cp .env.example .env
```

The defaults in `.env` work as-is for local play:

| variable | default | change it when |
|---|---|---|
| `DATABASE_URL` | `postgres://hollowrite:hollowrite@127.0.0.1:5432/hollowrite` | your database is elsewhere |
| `TEST_DATABASE_URL` | `…/hollowrite_test` | same |
| `HOLLOWRITE_BIND` | `127.0.0.1:7700` | port 7700 is taken, or for LAN play (section 8) |
| `HOLLOWRITE_SERVER` | `http://127.0.0.1:7700` | must match `HOLLOWRITE_BIND`; the web dev server and `make dev-terminal` use it |
| `HOLLOWRITE_REGISTRATIONS_PER_HOUR` | `10` | you are creating many test accounts from one machine |

`make` reads `.env` automatically. If you run commands by hand, export the
variables first (`set -a; source .env; set +a` in bash/zsh).

## 3. Start PostgreSQL

```bash
make db-up
```

This tries, in order:

1. **Docker Compose**: starts the `db` service (PostgreSQL 16, user and
   password `hollowrite`, databases `hollowrite` and `hollowrite_test`).
2. **A Debian/Ubuntu system cluster** (`pg_ctlcluster`): starts it and
   creates the role and both databases (uses `sudo -u postgres`, or
   `runuser` when you are root).
3. **A private cluster** in `.data/pg` using your local PostgreSQL binaries
   (macOS Homebrew and other non-Debian systems; not as root).

It is safe to run repeatedly. Expected last line, for example:

```
PostgreSQL 16/main ready on 127.0.0.1:5432
```

**Doing it by hand** (any OS, e.g. Windows with the PostgreSQL installer),
open `psql` as the `postgres` superuser and run:

```sql
create role hollowrite login password 'hollowrite' createdb;
create database hollowrite owner hollowrite;
create database hollowrite_test owner hollowrite;
```

## 4. Build, migrate, test

```bash
make web-install   # npm ci of the pinned web dependencies (~5 s)
make migrate       # first Rust build + schema; ~2–3 min the first time
make seed          # validates content; prints "content ok: 15 items, 4 enemies, 3 npcs, map 128x52 with 32 spawns"
make test          # Rust + web tests (database tests run too when TEST_DATABASE_URL is set)
```

`make test` should end with every `test result: ok.` line and
`Tests  12 passed (12)` for the web client. There are 74 Rust tests and 12
web tests in total; the slowest suites (playthrough, end-to-end) take about
10 seconds each.

Optional: `make lint` (rustfmt, clippy with warnings as errors, eslint).

Without make:

```bash
cd apps/web && npm ci && cd ../..
cargo run -p hollowrite-server -- migrate
cargo run -p hollowrite-server -- check-content
cargo test --workspace
cd apps/web && npm run typecheck && npm test
```

## 5. Run the game

```bash
make dev
```

This builds and starts the **game server** on `127.0.0.1:7700` and the **web
dev server** on `127.0.0.1:5173`, and prints:

```
server  http://127.0.0.1:7700   web  http://127.0.0.1:5173   terminal  make dev-terminal
```

Leave it running (Ctrl-C stops both). Without make, run these in two
shells instead:

```bash
cargo run -p hollowrite-server          # game server (needs DATABASE_URL)
cd apps/web && npm run dev              # web dev server, proxies to the game server
```

Readiness check from another shell:

```bash
curl http://127.0.0.1:7700/readyz     # {"database":true,"world":true}
```

### Browser player

Open <http://127.0.0.1:5173> (or `http://localhost:5173`; both are allowed
origins). Click **New account**, choose a name (3–16 letters, digits, `-`,
`_`, starting with a letter) and a password of at least 10 characters. Then
create a character: name, origin, sigil and colour, and click **Wake**.

### Terminal player

In a second terminal window, at least 80×24 (120×35 or larger looks best):

```bash
make dev-terminal
# without make: cargo run -p hollowrite-terminal -- --server http://127.0.0.1:7700
```

On the sign-in screen press **F2** to switch to *Create account*, type a
name, **Tab**, a password, **Enter**. Create a character (Tab moves between
fields, ◂ ▸ change origin/sigil/colour, Enter creates), then press **Enter**
on it to wake.

The terminal client remembers the session in
`~/.config/hollowrite/session.json` (mode 0600). Press **L** on the
character list to sign out, or start it with `--no-remember`.

Useful flags: `--ascii` (plain ASCII glyphs), `--colors 256|16`,
`--high-contrast`, `--reduced-motion`, `--input hold|repeat`,
`--server http://host:port`. Pass them via
`cargo run -p hollowrite-terminal -- --ascii` or the built binary
`target/debug/hollowrite --ascii`.

> **One login per account.** Signing the same account in twice moves the
> character to the newest client and the old one is told why. Use two
> different accounts for two players. Two browser tabs share one cookie, so
> for a second browser player use a private window or another browser.

### Controls

| key | action |
|---|---|
| `W A S D` / arrows | move; `Q E Z C` diagonals (browsers also accept two keys) |
| `J` / `K` | light / heavy attack |
| `Space` | dodge roll (invulnerable for a moment) |
| `L` | guard (hold; a toggle in terminals without key-release events) |
| `I` | parry (needs the Lantern Buckler, i.e. the Ash Warden) |
| `H` | drink a tallow draught |
| `1` / `2` | cast rites |
| `F` | talk, rest at a lantern, open doors, touch walls |
| `Tab` | gear, stats and quests (Enter equips, U uses, 2 = rite slot 2) |
| `V` | invest echoes (only beside a lantern you have lit) |
| `M` | map |
| `Enter` | chat; `/p text` party, `/invite name`, `/accept name`, `/leave`, `/mute name`, `/report name reason` |
| `Esc` | menu: glyphs, colours, contrast, motion, input mode, leave to character select |
| `?` | help and the telegraph legend |

Reading danger: an enemy about to attack shows `!` above it and turns amber.
Tinted cells mark exactly where the hit lands (`×` strike, `▓` heavy, `░`
area, `◌` shockwave, `≈` lingering pool) and brighten as it approaches. Roll
through it or step out.

## 6. Play-test checklist (two players)

Do these with one terminal player and one browser player. Each line says
what you should see.

**Meet and talk**

1. Both wake at the Last Lantern (`☼`). Each sees the other's glyph with a
   name label.
2. Terminal player presses Enter, types `hello`, Enter. The browser chat
   shows `Name: hello`, and the reverse works too.
3. Browser player types `/invite <terminal name>`. The terminal shows the
   invite; it types `/accept <browser name>`. Both side panels list the
   party, the partner gets a `+` after the name and an underline, and `/p
   text` reaches only the party.

**Tutorial and first quest**

4. Walk to Hesper (`Ħ`, west of the lantern), press **F**. Dialogue opens;
   the quest *A Second Light* appears under PATH.
5. Leave the sanctuary east, reach the crossroads, take either road (north:
   streets, south: orchard) to Gloam Plaza and press **F** at its lantern:
   **LANTERN LIT** banner.
6. From the plaza, walk west through the gap into the collapsed aqueduct
   and follow it to the iron door at its far end. Press **F** there: it only
   opens from this side, and from now on it is a short, enemy-free route
   between the crossroads and the plaza.

**Shared combat**

7. Go together to an Oathless Knight (`K`), e.g. in the streets. Watch the
   `!` and the tinted cells; roll through or step out. Both attack it.
8. When it dies, **both** players get `+60 echoes` once. Any drop (`♦`) is
   visible only to the player it was rolled for.
9. Ash Warden: press **I** just before a knight's cleave lands. `PARRY`
   appears, the knight is staggered, and your next hit is a critical.

**Death and remnants**

10. Let one player die. **UNTHREADED** appears, they wake at their last
    rested lantern, and the HUD shows `◊ remnant N <direction>`.
11. Walk back to the `◊`: **ECHOES RECLAIMED**. Die again before reclaiming
    and the earlier echoes are gone for good (by design).

**Progression**

12. With enough echoes (115 for level 2), stand beside a lit lantern, press
    **V** and invest a point. Health or stamina maxima change at once.
13. **Tab**: equip and unequip gear, use a consumable.

**Persistence**

14. Both players quit (terminal: Esc → Quit; browser: close the tab). Stop
    `make dev` with Ctrl-C, start it again, and sign back in. Level, echoes,
    gear, lit lanterns, opened doors, quest progress and position are all
    still there. Enemies are back, which is by design.

**The rest of the path (optional, 30–60 minutes)**

15. Hesper again → Ansel in the chapel ruin of the sanctuary → the Sexton's
    Key in the chapel of the Lampless Streets → the Ossuary gate in the
    plaza's east wall → the Sexton's Lantern → the Bell-Mother behind the
    fog gate. Enter the belfry together within five seconds: the door seals
    and both see the same boss bar. Below half health she cracks, rings
    shockwaves and drops hazard pools, then hangs open (**OPEN — STRIKE**).
16. The secret: somewhere along the orchard's southern wall, try **F**.

**Display modes**

17. Terminal `Esc` menu: toggle ASCII glyphs, cycle colours 24-bit / 256 /
    16, high contrast, reduced motion. Browser `Esc` menu: glyphs,
    contrast, reduced motion and text size. Gameplay information must stay readable in every mode.

## 7. Automated checks against a running server

With `make dev` (or any server) running:

```bash
make smoke      # two bots: register, meet, move, chat, party, ping; ends with "smoke passed"
make latency    # input-to-ack through a 150 ms RTT proxy; prints p50/p95/max
make load       # 20 wandering bots for 30 s, then server tick and snapshot metrics
```

`make load` creates 20 accounts from one IP; raise
`HOLLOWRITE_REGISTRATIONS_PER_HOUR` (and `HOLLOWRITE_CONNECT_BURST`) in
`.env` and restart the server first. Metrics are at
`http://127.0.0.1:7700/metrics` (loopback only).

The scripted two-client captures in `docs/captures/` can be reproduced with
`scripts/cross-client-fight.sh` (needs tmux and Playwright).

## 8. Playing on your LAN

For a friend on the same network. There is **no TLS** in this setup, so
passwords cross the network unencrypted; only do this on a network you
trust.

In `.env` on the host (replace `192.168.1.50` with your address):

```bash
HOLLOWRITE_BIND=0.0.0.0:7700
HOLLOWRITE_ALLOWED_ORIGINS=http://192.168.1.50:7700
HOLLOWRITE_SERVER=http://127.0.0.1:7700
```

Build the web client once and let the game server serve it:

```bash
make release
target/release/hollowrite-server
```

- Browser players open `http://192.168.1.50:7700`.
- Terminal players run `hollowrite --server http://192.168.1.50:7700`
  (the binary is `target/release/hollowrite`; copy it to their machine or
  build it there with `cargo build --release -p hollowrite-terminal`).
- Open port 7700 in the host firewall.

For anything beyond a LAN, put a TLS reverse proxy in front and follow
`docs/OPERATIONS.md` and the gaps listed in `docs/SECURITY.md`.

## 9. Resetting

Delete all accounts and characters and start over:

```bash
# native PostgreSQL
psql -h 127.0.0.1 -U hollowrite -d postgres -c "drop database hollowrite" -c "create database hollowrite"
make migrate

# Docker
docker compose down -v && make db-up && make migrate
```

Forget the terminal client's saved login: `rm ~/.config/hollowrite/session.json`
(or press **L** on the character list).

## 10. Troubleshooting

| symptom | cause and fix |
|---|---|
| `make db-up`: "neither Docker nor PostgreSQL binaries found" | Install one of them (section 1). On macOS make sure `$(brew --prefix postgresql@16)/bin` is on `PATH`. |
| server log: `DATABASE_URL must be set` | You ran the binary directly; `cp .env.example .env` and use `make`, or export the variables. |
| server: `connection refused` to 5432 | PostgreSQL is not running: `make db-up`. Another PostgreSQL on 5432 with different credentials: change `DATABASE_URL`. |
| `address already in use` on 7700 or 5173 | Something else is listening. `make check-env` shows which port. Change `HOLLOWRITE_BIND` and `HOLLOWRITE_SERVER` together, or stop the other process. |
| browser: "Disconnected: …" right after sign-in, server log says `websocket from disallowed origin` | You opened the page on a host/port that is not in the allow-list (e.g. a LAN IP). Add it to `HOLLOWRITE_ALLOWED_ORIGINS` and restart the server. |
| registration: "too many registrations from this address" | Raise `HOLLOWRITE_REGISTRATIONS_PER_HOUR` and restart. |
| login: "account temporarily locked" | Five wrong passwords in a row; wait five minutes. |
| a client says "You logged in from another client" | The same account signed in elsewhere; use separate accounts per player. |
| terminal: "HOLLOWRITE needs at least 80x24" | Enlarge the window or reduce the font size. |
| terminal: boxes, question marks or misaligned walls | Your font lacks the glyphs, or your locale treats them as double width. Use a font like DejaVu Sans Mono, Cascadia Mono or JetBrains Mono, or run with `--ascii`. |
| terminal: colours look wrong or washed out | Try `--colors 256` or `--colors 16`; set `COLORTERM=truecolor` if your terminal supports 24-bit colour. |
| terminal: movement keeps going briefly after releasing a key | Your terminal does not report key releases, so the client holds each press slightly longer than the OS repeat delay. Terminals with the kitty keyboard protocol (kitty, WezTerm, foot, Ghostty, recent Alacritty) give exact holds. `--input repeat` forces the fallback, `--input hold` forces release events. |
| terminal: guard (`L`) stays on | Without key-release events guard is a toggle; press `L` again. |
| browser: keys do nothing | The chat box has focus; press Esc or click the game view. |
| `npm ci` fails | Node older than 20; upgrade. Delete `apps/web/node_modules` and retry. |
| first `cargo` build is slow | Expected: about 2–3 minutes for everything on 4 cores; later builds are incremental. |
