# ADR 0002: Monolith-first, single authoritative world actor

- Status: accepted
- Date: 2026-10-11

## Context

The brief asks for a persistent shared world, server authority over every
outcome, and boundaries that allow zones, instances and horizontal scaling
later, without paying for microservices now.

## Decision

- One server process. One Tokio task (the world actor) exclusively owns the
  simulation, all connected players, parties and chat routing.
- Connections are separate tasks that only exchange bounded messages with
  the actor. Persistence is a separate single-writer task.
- `game-core` is a library with no I/O, so the same `World` can later run per
  zone or per instance without changes.
- No Redis, no message broker, no service mesh in v0.

## Alternatives considered

- **Lock-based shared state** (`Arc<Mutex<World>>` touched by connection
  tasks): more contention, and correctness of loot/currency would depend on
  lock discipline everywhere. Rejected.
- **ECS framework** (bevy_ecs, hecs): useful at larger entity counts; the
  slice has tens of entities and benefits more from plain structs and
  readable code. Revisit with load data.
- **Separate gateway and zone services from day one**: more moving parts than
  the slice needs; the message boundaries are already where a split would go.

## Consequences

- Atomicity of rewards and pickups comes for free.
- One slow tick delays everyone; tick time is measured and exported
  (`hollowrite_tick_duration_us`, `hollowrite_tick_overruns_total`).
- Capacity is bounded by one core for the simulation. Measured tick times are
  in TEST_PLAN.md; the path beyond one process is in ARCHITECTURE.md.
