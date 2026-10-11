# Milestones, risks and limitations

Status as of 2026-10-11. "Done" means implemented and covered by tests or a
recorded manual check; see TEST_PLAN.md for the evidence.

## M0: reconnaissance and design lock (done)

- [x] Repository inspected (fresh: README only); toolchain checked (Rust
      1.97, Node 22, PostgreSQL 16 available, Docker daemon not available in
      the build environment).
- [x] Design, architecture, protocol, content and security documents.
- [x] ADRs: wire protocol (0001), monolith and world actor (0002),
      character aggregate persistence (0003), terminal input (0004).
- [x] Dependencies pinned (`Cargo.lock`, `package-lock.json`, exact npm
      versions).

## M1: shared online skeleton (done)

- [x] Cargo workspace, web app, migrations, health and readiness checks.
- [x] Versioned handshake, Argon2id accounts, sessions, character creation
      and selection.
- [x] Terminal and browser clients connected to one server, rendering the
      same map and each other.
- [x] Boot scripts, `make dev`, two-client smoke bot.

## M2: actual game loop (done)

- [x] Server-owned movement, collision, interest management.
- [x] Combat state machine: light/heavy, roll with i-frames, guard, parry and
      riposte, stagger and poise, draughts, rites.
- [x] Enemy AI (chase, ranged spacing, blink, leash, respawn), equipment,
      items, levelling, lanterns, checkpoint respawn, remnants.
- [x] Persistence with versioned saves and ledger; reconnect and restart.
- [x] Tests for damage authority, stamina, duplicate rewards, reconnect.

## M3: authored content and boss (done, needs playtesting)

- [x] Gloamreach (streets, orchard, plaza), the Ossuary and the belfry, with
      two shortcuts and a secret.
- [x] Three enemy archetypes; the Bell-Mother with two phases, shockwaves,
      hazards and a vulnerability window; co-op scaling and a sealed shared
      arena.
- [x] Three NPCs, two quests with an exclusive ending choice.
- [x] The whole chain is completed by an automated playthrough.
- [ ] Human playtests of the 30–60 minute target and of the boss difficulty.

## M4: visual and UX polish (mostly done)

- [x] Style guide, shared theme, lighting and fog of war, animated tiles,
      telegraph overlays, hit flashes and numbers, swing arcs, banners.
- [x] Responsive terminal layout (80×24 compact, 110×30+ with panels);
      browser layout with accessible DOM panels.
- [x] ASCII, 256 and 16 colour, high contrast, reduced motion, text size.
- [x] Network indicator and stale-connection warning.
- [ ] Audio cues (optional in the brief; not implemented).
- [ ] Pointer interactions in the browser (keyboard only for now).
- [ ] Captures of combat across both clients at once (rules are tested;
      screenshots exist only for the shared sanctuary).

## M5: hardening (in progress)

- [x] Security review of the implemented surface (SECURITY.md), rate limits,
      origin checks, CSRF header, sanitising, moderation CLI with audit log.
- [x] Migration from empty database, backup/restore drill, graceful shutdown.
- [x] CI workflow, `cargo audit` and `npm audit` clean.
- [x] Load and latency measurements with real bots and a delaying proxy.
- [ ] Windows/macOS terminal testing, Firefox/Safari testing.
- [ ] Docker image build verification (no daemon here).
- [ ] Public-deployment items listed in SECURITY.md (TLS setup, e-mail,
      password reset, privacy notice, backups automation).

## Risk register

| risk | likelihood | impact | mitigation in place | next step |
|---|---|---|---|---|
| Terminal input portability: no key-release events in most terminals | high | movement feels sticky | kitty protocol detection; repeat-based hold emulation; explicit diagonals; guard toggle; selectable mode; server-side movement TTL | test on Windows Terminal, macOS Terminal and iTerm2; tune hold durations per platform if needed |
| Real-time feel over WebSocket/TCP at 100–200 ms RTT | medium | combat feels late | generous telegraphs (12–34 ticks), local movement prediction with reconciliation, 110 ms interpolation, measured 201 ms p50 input→ack at 150 ms RTT | playtest at 150 ms; consider sending snapshots at 30 Hz to players in combat |
| Cross-client glyph layout: font fallback, ambiguous width | medium | misaligned or doubled glyphs | single-width glyph test, ASCII mode, browser centres each glyph in a fixed cell and strokes walls instead of relying on box-drawing glyphs | test common fonts on Windows and macOS; East Asian locales should use `--ascii` |
| Snapshot size with crowds | medium | bandwidth | interest radius, measured 5–13 KB | field deltas, then binary encoding, guided by measurements |
| Single-process world | low (for v0) | one slow tick delays all | tick metrics, overrun counter | zone split as in ARCHITECTURE.md |
| Boss balance untested by humans | high | frustrating or trivial | tunables in content, co-op scaling, scripted tests only cover regular enemies | playtest and adjust `enemies.toml` |
| Name collision with an existing "HollowRite" community | known | branding | working title, renaming notes in ARCHITECTURE.md | trademark and name check before any public launch |

## Known limitations

- One region, one boss, two quests. No faction effects, trading, PvP,
  invasions, instances, crafting or audio.
- Parties and boss encounters are in memory and reset on restart.
- Enemies reset on server restart; there is no persistent world damage.
- Moderation is CLI only.
- Accounts cannot recover a forgotten password.
- Rate limiters are per process and keyed by peer address (no proxy
  header support yet).
- Mobile and touch input are not supported; the browser says so.

## Next steps, in order

1. Human playtest session (two people, one per client) through the whole
   path; record session length, deaths and the boss fight.
2. Capture cross-client combat and the boss for the README.
3. Windows and macOS terminal checks; Firefox and Safari checks.
4. Field-delta snapshots behind a protocol version bump, if measurements
   from playtests show the need.
5. Deployment profile: TLS reverse proxy config, systemd unit, nightly
   backups, trusted proxy headers for rate limits.
