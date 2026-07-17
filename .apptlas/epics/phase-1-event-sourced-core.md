# Epic: Phase 1 — Event-sourced core

- **State:** next up
- **Implements:** design spec §3 (event model, delivery semantics, hot path), §6 phase 1
- **Exit criteria:** test rings 1–3 green (ring 4 arrives with phase 2);
  enqueue→lease→ack/fail→retry→park works through use cases against both
  stores; every projection replays from event zero to an identical state.

## Candidate items

1. **Domain value objects & identifiers** — `JobId`, `QueueName`, `LeaseId`,
   `Attempt`; validation invariants. `Queue` starts as configuration + value
   object here; it grows into the full aggregate spec §2 names only when
   per-queue policies demand it (recorded disposition, roadmap review
   2026-07-17). `WorkerRegistration` is deferred to phase 2, where workers
   first connect (same disposition). *(ring 1)*
2. **`JobEvent` taxonomy v1** — the full spec §3 set including reserved
   durable-execution kinds; every event carries `correlation_id`,
   `causation_id`, W3C traceparent. Serialization schema decided here
   (ADR: event payload encoding + versioning). *(ring 1)*
3. **`Job` aggregate + state machine** — fold over events; transition table
   exactly as spec §3 diagram; illegal transitions are typed errors.
   **proptest:** no event sequence reaches an illegal state. *(ring 1)*
4. **`RetryPolicy`** — exponential backoff + jitter as a pure, deterministic
   function of (attempt, policy, seed); park on exhaustion. *(ring 1)*
5. **Ports** — `EventStore` (append w/ optimistic concurrency, read stream),
   `OutboxRelay`, `ProjectionStore`, `LeaseManager`, `Clock`, `IdGenerator`;
   signatures generic over aggregate/event (the kineticrs lesson).
   Snapshots (spec §2) are deferred until phase-2 benchmarks show fold cost —
   but the `EventStore` port shape must not preclude adding them (recorded
   disposition, roadmap review 2026-07-17). *(compile)*
6. **Use cases** — `EnqueueJob`, `LeaseNextJob`, `AckJob`, `FailJob` (retry
   or park), `ExtendLease` (heartbeat), `CancelJob`, `SweepExpiredLeases`;
   late-ack conflict event path. *(ring 2 against store-memory)*
7. **`koine-store-memory`** — complete port implementations. *(ring 2 proof)*
8. **Postgres migrations** — `event_store.events` (append-only, unique
   `(stream_id, version)`), `dispatch_queue`, `outbox`,
   `projection_positions`. *(ring 3 via `sqlx::migrate!`)*
9. **`koine-store-postgres`** — event store (optimistic concurrency via
   unique violation), dispatch projection updated in the append transaction,
   `SELECT … FOR UPDATE SKIP LOCKED` lease claim, outbox relay with
   persisted positions. *(ring 3)*
10. **Crash-scenario integration tests** — worker dies → lease expires →
    retry; late ack records conflict; relay restart resumes from position;
    projection full replay. *(ring 3)*
11. **End-to-end product exercise** — a `koine-server` dev command (or
    example bin) driving the full cycle against real Postgres; also serves
    DoD item 2 for the phase. *(manual + ring 3)*
12. **Wiki pages** — `koine-domain`, `koine-application`, `koine-store-*`,
    plus an `event-model.md` page. *(DoD)*
13. **Kani pilot (stretch)** — evaluate proving one state-machine invariant;
    record findings either way.

## Dependencies

- Docker (testcontainers) locally and in CI — CI needs a service/DinD
  decision (small CI change, decide in the plan).
- ADR needed at plan time: event payload encoding (JSON vs JSONB layout,
  schema-version field), naming of streams.

## Risks

- Event schema decisions are the hardest to change later — spend design time
  in the plan's first tasks, not mid-implementation.
- Postgres perf assumptions (SKIP LOCKED under contention) unmeasured until
  phase 2 benchmarks — keep the dispatch table narrow.

## Verification strategy

Rings 1–3 (testing-policy); proptest on the state machine is non-negotiable;
Kani is stretch. Phase closes only with the epic's exit criteria in the epic
file's evidence, per DoD.
