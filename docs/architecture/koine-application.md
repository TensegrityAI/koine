# `koine-application`

## What it does

The application layer: driven ports the domain needs from the outside world,
and the use cases that orchestrate `koine-domain` commands against them.
Use cases are thin — validation and state-machine logic live in `Job`; this
crate's job is to load, append, and wrap events with lineage.

## How it is built

- **Ports (`src/ports.rs`)** — `EventStore` (`append`, `load`), `Dispatcher`
  (`lease_next`, `extend_lease`, `expired`), `Clock` (`now`), `IdGenerator`
  (`job_id`, `event_id`, `lease_id`, `correlation_id`, `jitter_seed`). All
  four are `Send + Sync` traits using native async-fn-in-trait
  (`-> impl Future<Output = ...> + Send`), so calls dispatch statically with
  no boxed futures.
- **Composite-operation contracts live in the adapter, not the use case**
  (ADR 0006/0011): `EventStore::append` must update the dispatch index
  atomically with the write; `Dispatcher::lease_next` must atomically pick
  the next eligible job, produce `leased` via the domain aggregate, append
  it, and update the index. Use cases call these ports and stay ignorant of
  how atomicity is achieved.
- **`wrap_events` (`src/lineage.rs`)** — every use case builds its event(s)
  through `Job`'s command methods, then calls `wrap_events(ids, clock,
  stream, base_version, correlation_id, causation_id, traceparent, events)`
  to produce sequential `EventEnvelope`s: one shared `recorded_at`, fresh
  `event_id` per event, versions starting at `base_version + 1`.
- **Use cases (`src/use_cases/`)**:
  - `enqueue::EnqueueJob` — opens a new stream with `Enqueued` at version 1;
    mints a correlation id if the caller didn't supply one via `Lineage`.
  - `worker_ack::WorkerAck` — `start`, `succeed`, `fail`. A stale ack (the
    domain command fails because the lease no longer matches) is caught and
    turned into a `late_ack_conflict` record instead of propagating as an
    error — `AckOutcome::Recorded` vs. `AckOutcome::Conflict` tells the
    caller which happened.
  - `cancel::CancelJob` — appends `Cancelled` in any non-terminal state.
  - `lease::LeaseNextJob` — a direct pass-through to `Dispatcher::lease_next`.
  - `heartbeat::Heartbeat` — a pass-through to `Dispatcher::extend_lease`;
    writes no event (ephemeral, ADR 0011-c).
  - `sweep::SweepExpiredLeases` — lists `Dispatcher::expired(now)`, folds
    each stream, emits `lease_expired` + the retry decision. Races (a job
    acked between listing and folding) are skipped via `Job::expire_lease`
    returning an error, or the store rejecting with `VersionConflict` — the
    next sweep sees the truth.
- **Lineage plumbing** — `worker_ack`, `cancel`, and `sweep` all derive
  `(correlation_id, causation_id, traceparent)` from the loaded stream:
  correlation is carried from the stream's first envelope (set at
  `enqueued`), causation is the last envelope's `event_id`.

## Why

- ADR 0006 — dispatch projection is synchronous with the append; that
  contract is why `EventStore` and `Dispatcher` are separate ports the use
  cases trust rather than orchestrate.
- ADR 0011 — names exactly which composite operations the adapter owns
  ((a) append + index update, (b) claim + append + index update) and that
  lease extension is ephemeral and event-free.

## Boundaries

- Depends on `koine-domain` only.
- Defines the ports; `koine-store-memory` (today) and `koine-store-postgres`
  (phase 1B) implement them. Driving adapters (`koine-grpc`, `koine-http`,
  `koine-mcp`, `koine-cli`) call the use cases in this crate.
- `OutboxRelay` and `ProjectionStore` ports — for the async projection tier
  (history, metrics, dashboard) — are not implemented yet; they arrive with
  phase 1B alongside the Postgres adapter.
