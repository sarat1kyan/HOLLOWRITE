# Contributing

## Ground rules

- The server is the only authority. Clients may predict and animate; they
  never decide damage, loot, positions or progression.
- `game-core` stays free of I/O, async runtimes and wall-clock time. If a
  rule needs randomness, use the world's seeded RNG.
- Gameplay numbers belong in `content/`, not in code.
- Every gameplay change comes with a deterministic test in
  `crates/game-core/tests/`; every protocol change regenerates the fixtures
  (`HOLLOWRITE_WRITE_FIXTURES=1 cargo test -p hollowrite-protocol`) and
  updates `apps/web/src/protocol.ts` and NETWORK_PROTOCOL.md.
- Schema changes are new numbered files in `db/migrations`; never edit an
  applied migration.
- Keep both clients in step: a new entity, tile or telegraph needs theme
  entries and rendering in both `crates/terminal-client/src/view.rs` and
  `apps/web/src/game/render.ts`.

## Before opening a pull request

```bash
make lint
make test
make test-db      # with TEST_DATABASE_URL set
```

and, for anything touching networking, `make dev` plus `make smoke`.

## Style

- Rust: `rustfmt` (max width 120), `clippy -D warnings`.
- TypeScript: strict mode, ESLint config in `apps/web/eslint.config.js`;
  never render HTML from strings.
- Comments explain why, not what.

## Content contributions

Original work only. Do not add names, maps, items, dialogue or imagery from
existing games or other copyrighted works. Run
`cargo run -p hollowrite-server -- check-content` and the content tests after
any edit under `content/`.
