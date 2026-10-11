# ADR 0001: JSON over WebSocket for the v0 wire protocol

- Status: accepted
- Date: 2026-10-11

## Context

Two very different clients (a Rust terminal app and a browser) must speak to
the same server. The slice needs real-time movement and combat at 30 Hz
simulation and roughly 15 Hz snapshots, chat, and a handshake that rejects
mismatched clients. Debuggability matters more than bytes at this stage.

## Decision

- Transport: WebSocket over TCP (TLS in deployment). Browsers support nothing
  lower-latency without WebRTC, and one transport for both clients halves the
  surface area.
- Encoding: one JSON object per text frame, internally tagged by `t`, types
  defined once in `crates/protocol` with serde and mirrored in
  `apps/web/src/protocol.ts`. Unknown variants and unknown enum values are
  rejected, so a client cannot invent commands.
- Versioning: `PROTOCOL_VERSION` is checked in the `hello` exchange; a
  mismatch gets `protocol_mismatch` and the socket closes. Additive optional
  fields do not bump the version.
- Snapshots are full interest-set states plus `gone` ids rather than field
  deltas.
- Drift between Rust and TypeScript is caught by `crates/protocol/tests/
  fixtures.rs`, which regenerates or verifies `apps/web/src/__fixtures__/
  protocol.json`, consumed by the web tests.

## Consequences

- Snapshots measured at about 2–5 KB in crowded spots (TEST_PLAN.md). That is
  acceptable for the slice and becomes the first optimisation target: field
  deltas, then a binary codec, only with measurements in hand.
- TCP head-of-line blocking means a lost packet delays everything behind it.
  Acceptable for PvE with interpolation and generous telegraphs; not suitable
  for competitive PvP, which is out of scope.
- Every frame is human-readable in browser devtools and logs, which made the
  slice much faster to build and debug.
