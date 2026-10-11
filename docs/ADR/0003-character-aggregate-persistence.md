# ADR 0003: Persist characters as versioned aggregates

- Status: accepted
- Date: 2026-10-11

## Context

Currency, items and the dropped remnant must never be duplicated by crashes,
reconnects or concurrent saves, and the database must not be written every
tick.

## Decision

- The in-memory `CharacterData` is the unit of persistence. Every save writes
  the whole aggregate (row plus child tables) in one transaction.
- Each save carries a monotonically increasing `version`; the update only
  applies when the stored version is lower.
- A single persistence task processes saves and loads in FIFO order.
- An `echo_ledger` row is appended in the same transaction for every
  economic change since the previous save (kills, boss rewards, consumables,
  dialogue rewards, remnant drop/loss/reclaim, levelling).
- Saves are triggered by meaningful events and every 30 s, never per tick.

## Consequences

- Crash recovery may roll a character back by up to one save interval but
  never forward into an impossible state.
- The ledger is an audit trail, not the source of truth; the source of truth
  is the aggregate. This keeps reads simple and avoids replay logic.
- Writes are slightly larger than incremental updates (child tables are
  rewritten). With small inventories this is cheap; measured save latency is
  exported as `hollowrite_db_latency_us`.
