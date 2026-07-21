# `koine-application`

## What it does

The application layer: driven ports the domain needs from the outside world,
and the use cases that orchestrate `koine-domain` commands against them.
Use cases are thin — validation and state-machine logic live in `Job`; this
crate's job is to load, append, and wrap events with lineage.

## How it is built

- **Ports (`src/ports.rs`)** — `EventStore` (`append`, `load`), `Dispatcher`
  (`lease_next`, `extend_lease`, `retire_next_expired_lease`), `EventSink` (`deliver`) — the
  outbox consumer port (ADR 0012), fed by an adapter's relay loop, with
  `SinkError` (a failed batch) and `RelayError` (`Sink` plus adapter
  `Backend` failure) covering the failure modes — `Clock` (`now`),
  `IdGenerator` (`job_id`, `event_id`, `lease_id`, `correlation_id`,
  `jitter_seed`). Phase 2A added two more: `DispatchSignal` (`notify(queue)`,
  `wait(queue, timeout)`) — the wakeup channel `koine-grpc`'s `Fetch` stream
  waits on instead of polling hot; spurious wakeups are allowed and a
  `notify()` racing ahead of a `wait()` may be missed, so callers rely on
  the timeout backstop and re-check by claiming — and `WorkerPresence`
  (`seen(worker, queue)`) — ephemeral liveness tracking with no domain
  event (ADR 0015). All seven are `Send + Sync` traits using native
  async-fn-in-trait (`-> impl Future<Output = ...> + Send`), so calls
  dispatch statically with no boxed futures.
- **Composite-operation contracts live in the adapter, not the use case**
  (ADRs 0006, 0011, and 0016): `EventStore::append` must update the dispatch index
  atomically with the write; `Dispatcher::lease_next` must atomically pick
  the next eligible job, produce `leased` via the domain aggregate, append
  it, and update the index. `Dispatcher::retire_next_expired_lease` must
  select and revalidate one expired live grant, derive `Job::expire_lease`
  events, append them, and update the index in that same transaction or
  critical section. Use cases call these ports and stay ignorant of how
  atomicity is achieved. This strengthens only the internal port: the event
  taxonomy and the public `koine.v1` wire contract are unchanged.
- **`wrap_events` (`src/lineage.rs`)** — every use case builds its event(s)
  through `Job`'s command methods, then calls `wrap_events(ids, clock,
  stream, base_version, correlation_id, causation_id, traceparent, events)`
  to produce sequential `EventEnvelope`s: one shared `recorded_at`, fresh
  `event_id` per event, versions starting at `base_version + 1`.
- **Use cases (`src/use_cases/`)**:
  - `enqueue::EnqueueJob` — opens a new stream with `Enqueued` at version 1;
    mints a correlation id if the caller didn't supply one via `Lineage`;
    validates the `RetryPolicy` against sane bounds first (`max_attempts` in
    `1..=10_000`, `base_delay <= max_delay`, `max_delay` up to 30 days),
    rejecting a pathological policy as `EnqueueError::InvalidPolicy` at the
    enqueue boundary rather than letting it reach the domain.
  - `worker_ack::WorkerAck` — `start`, `succeed`, `fail`. A stale ack (the
    domain command fails because the lease no longer matches) is caught and
    turned into a `late_ack_conflict` record instead of propagating as an
    error — `AckOutcome::Recorded` vs. `AckOutcome::Conflict` tells the
    caller which happened.
  - `cancel::CancelJob` — appends `Cancelled` in any non-terminal state.
  - `lease::LeaseNextJob` — a direct pass-through to `Dispatcher::lease_next`.
  - `heartbeat::Heartbeat` — a pass-through to `Dispatcher::extend_lease`;
    writes no event (ephemeral, ADR 0011-c).
  - `sweep::SweepExpiredLeases` — loops
    `Dispatcher::retire_next_expired_lease` until it returns `None`. The
    adapter, rather than the use case, owns candidate selection, deadline
    revalidation, aggregate expiry/retry derivation, append, and projection
    update. A dispatcher error surfaces as `SweepError::Dispatch`; it is
    never reported as a partial successful retirement.
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
- ADR 0013 — `DispatchSignal` exists so `koine-grpc`'s `Fetch` stream can
  wake on new work instead of polling a drained queue.
- ADR 0015 — `WorkerPresence` is ephemeral infrastructure state, like lease
  deadlines (ADR 0011-c): no domain event, no aggregate, no stream.
- ADR 0016 — expiry and heartbeat serialize on the same lease state: if the
  heartbeat wins, retirement observes its extended deadline and does not
  expire the grant; if retirement wins, a later heartbeat returns `false`.
  The formal recovery liveness claim is conditional on a finite heartbeat
  bound: a worker may validly renew forever in the production protocol.

## Boundaries

- Depends on `koine-domain` only.
- Defines the ports; `koine-store-memory` and `koine-store-postgres`
  implement all seven (including the phase-2A `DispatchSignal`/
  `WorkerPresence` pair — `NotifySignal`/`NoopPresence` and `PgSignal`/
  `PgPresence` respectively). Driving adapters (`koine-grpc` since phase
  2A; `koine-http`, `koine-mcp`, `koine-cli` still stubs) call the use
  cases in this crate.
- The outbox consumer port shipped in phase 1B as `EventSink` (superseding
  this page's earlier `OutboxRelay` placeholder name); a `ProjectionStore`
  port for the async projection tier (history, metrics, dashboard) is still
  not implemented — deferred to phase 3's real read projections.
