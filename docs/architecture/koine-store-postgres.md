# `koine-store-postgres`

## What it does

The production driven adapter: `PostgresEventStore` (`EventStore`),
`PostgresDispatcher` (`Dispatcher`), `PostgresOutboxRelay` (delivers to an
`EventSink`), `PgSignal` (`DispatchSignal`, phase 2A), and `PgPresence`
(`WorkerPresence`, phase 2A), plus `rebuild_dispatch` (replay-from-zero ops
tool) and `connect_pool` (the one entry point composition roots use —
connects and runs the embedded `sqlx::migrate!` migrations). It is the
durable twin of `koine-store-memory`: the ADR 0011 composite contracts and
  the ADR 0016 atomic retirement contract, with a real transaction standing
  in for the mutex guard.

## How it is built

- **Schema** (`migrations/0001_event_store.sql`, `event_store` schema) —
  `events` (append-only; `global_seq` identity gives total order;
  `UNIQUE (stream_id, version)` is the optimistic-concurrency guard; envelope
  fields decomposed into indexed columns plus `payload JSONB`); `dispatch_queue`
  (one row per dispatchable/leased job; `seq` from a dedicated sequence, minted
  once, preserved across updates); `outbox` (`outbox_seq` identity, full
  envelope `JSONB` for cheap relay delivery). Two partial indexes carry the
  hot paths: `dispatch_claim_idx` on `(queue, priority DESC, seq) WHERE
  lease_id IS NULL` (the claim) and `dispatch_expiry_idx` on `(lease_expires_at)
  WHERE lease_id IS NOT NULL` (the sweep). **`migrations/0002_worker_presence.sql`**
  (phase 2A) adds `event_store.workers` (`worker_id` PK, `first_seen`,
  `last_seen`, `last_queue` — ADR 0015): no indexes beyond the primary key,
  since it's read by ad hoc operator queries (`WHERE last_seen > now() -
  interval '1 minute'`), not a hot path.
- **`PgSignal` notifies in-transaction** (`store.rs::project_in_tx`) — every
  time a job's dispatch row is (re-)inserted into `Pending` (fresh enqueue or
  a retry landing back in `Pending`), the same transaction runs `SELECT
  pg_notify('koine_dispatch', $1)` with the queue name as payload — so a
  waiting `Fetch` stream only wakes when there is actually new claimable
  work, and only once the transaction that created it commits. `PgSignal::
  wait` (`signal.rs`) opens a `PgListener`, `LISTEN`s on `koine_dispatch`,
  and loops `recv()` until a notification for the right queue arrives or the
  timeout elapses — the **entire** operation (pool acquire, listen, recv
  loop) is wrapped in one outer `tokio::time::timeout`, not just the
  individual `recv()` calls, so a slow pool acquire can't make the caller
  wait past its budget (a fix made during review). Contrast with
  `koine-store-memory`'s `NotifySignal`: the memory store's `append` never
  calls `notify` itself, only this crate's does — a caller building a
  memory-backed `Fetch` stream must signal manually (documented on that
  page).
- **`PgPresence` is best-effort by design** (`presence.rs`) — `seen` upserts
  `event_store.workers` (`last_seen = now()`, `last_queue` via `COALESCE`
  so a call with `queue: None` doesn't clobber the last known queue) and
  silently swallows any DB error: presence tracking must never fail or slow
  down a worker's request (ADR 0015). There is no retry, no logging, no
  propagated failure — a dropped presence update is invisible by design,
  not a bug.
- **Append is one transaction** (`store.rs::append_in_tx`) — explicit
  `SELECT max(version)` against the expected version (races resolved by the
  unique-constraint mapping Postgres error `23505` on
  `events_stream_version_unique` to `EventStoreError::VersionConflict`), then
  per-envelope event + outbox inserts, then a re-fold of the whole stream
  (`Job::from_events`) and `project_in_tx` — the dispatch row is re-derived
  from the *folded* aggregate, not from matching the just-appended event
  variant, so it is a rebuildable projection exactly like the memory store's
  `project_locked`. A failed transaction leaves nothing: verified for both a
  fresh stream (illegal opener) and an existing one (illegal continuation,
  prior events survive) by `failed_append_leaves_no_trace_fresh_or_existing`.
- **Claim is one transaction** (`dispatcher.rs::claim`) — `SELECT job_id FROM
  dispatch_queue WHERE queue = $1 AND lease_id IS NULL AND (not_before IS NULL
  OR not_before <= $2) ORDER BY priority DESC, seq LIMIT 1 FOR UPDATE SKIP
  LOCKED`, fold the picked stream, produce `leased` via the domain aggregate
  (`Job::lease` — domain validation stays authoritative, ADR 0011-b), append
  it through the same `append_in_tx` the event store uses, commit. Concurrent
  claimers never collide: `concurrent_claims_get_distinct_jobs`.
- **Heartbeat and retirement serialize in one lease-row transaction**
  (`dispatcher.rs`) — `extend_lease` updates the matching, still-live
  `dispatch_queue` row to a fresh `now + ttl` deadline. A
  `retire_next_expired_lease` transaction selects one expired row in deadline/
  job order with `FOR UPDATE SKIP LOCKED`, revalidates the current row, folds
  the stream, derives `Job::expire_lease` events, appends them through the
  transaction-local append machinery, and updates the projection before
  commit. If heartbeat commits first, retirement sees the extended deadline
  and skips that grant; if retirement commits first, the row no longer holds a
  live matching lease and heartbeat returns `false`. The change is internal:
  heartbeats remain event-free, expiry/retry events retain their taxonomy and
  lineage, and the public `koine.v1` wire contract is unchanged.
- **Outbox relay is claim-delete, not positions** (`relay.rs`) — `SELECT …
  ORDER BY outbox_seq LIMIT $n FOR UPDATE SKIP LOCKED`, deliver the ordered
  batch to an `EventSink`, delete on success; sink failure rolls the
  transaction back so the batch is redelivered later
  (`relays_in_order_and_deletes_on_success`,
  `sink_failure_rolls_back_for_redelivery`). Per-stream ordering holds with a
  single relay instance — all 1B needs (ADR 0012).
- **`rebuild_dispatch`** — folds every stream in first-appearance order
  (`GROUP BY stream_id ORDER BY min(global_seq)`) and re-projects each one,
  proving the dispatch table is derived state: `TRUNCATE dispatch_queue` then
  rebuild lands on byte-identical rows
  (`dispatch_queue_rebuilds_identically_from_the_log`). Run only against
  quiesced writers (maintenance window): under concurrent claims this upsert
  can overwrite a fresh lease from a stale fold, re-exposing a leased job.
- **Runtime queries** (`sqlx::query`/`query_as`, never `query!`) — no
  build-time `DATABASE_URL`, no offline-cache drift; the ring-3 suite against
  real migrations (never an inline schema copy) is the correctness gate.

## Why

- ADR 0005 — Postgres behind the `EventStore` port; the memory store keeps
  the port honest rather than secretly Postgres-shaped.
- ADR 0006 — dispatch is synchronous with the append (the hot path); every
  other projection is async via the outbox.
- ADR 0011 — names the two composite contracts this crate is the production
  proof of: (a) append + dispatch-index update, (b) claim + append +
  dispatch-index update; lease extension stays ephemeral.
- ADR 0012 — the schema shape, the append/claim transaction mechanics, why
  the relay is claim-delete instead of position-tracking, and why queries are
  runtime, not compile-time-checked.
- ADR 0013 — `PgSignal` is the production `DispatchSignal`: Postgres
  `LISTEN`/`NOTIFY` is what lets `koine-grpc`'s `Fetch` stream push instead
  of poll.
- ADR 0015 — `PgPresence`/`event_store.workers` is the durable half of
  ephemeral worker presence: no domain event, no aggregate, survives
  restarts as rows filtered by `last_seen`.
- ADR 0016 — the retirement transaction and heartbeat update fence each
  other on the current lease row while retaining `SKIP LOCKED` concurrency
  for unrelated jobs. Formal recovery liveness is conditional on the model's
  finite heartbeat allowance; production workers may renew forever.

## Boundaries

- Depends on `koine-application` (implements its ports) and `koine-domain`
  (folds `Job`, emits `JobEvent`); no crate above it in the hexagon may bypass
  these ports to reach `sqlx` directly (ADR 0003).
- Requires Postgres — exercised at 11 (testcontainers-modules' default image,
  ring 3) through 17 (the `koine-server dev-loop`/`serve` manual runs); the
  schema's floor is native `GENERATED ALWAYS AS IDENTITY` columns (PG 10+).
- `koine-store-memory` is the behavioral twin: the crash-recovery lifecycle
  suite (`tests/lifecycle.rs`) mirrors `koine-store-memory`'s ring-2 story
  test-for-test against real SQL (ring 3); `tests/signal.rs` (phase 2A)
  covers `PgSignal`/`PgPresence` the same way (`signal_wait_wakes_on_
  append_to_queue`, `signal_wait_on_other_queue_times_out`,
  `presence_records_worker_with_queue`).
- Consumed (phase 2A) as a `[dev-dependencies]` of `koine-grpc`, whose
  `tests/grpc_e2e.rs` binds a real `tonic` server to a real TCP port over
  this crate's Postgres adapters — the only suite in the workspace
  exercising the gRPC surface against real transport *and* real Postgres
  simultaneously.
- The outbox relay is single-instance by design (ADR 0012); consumer
  positions and relay concurrency are deferred to phase 3's real read
  projections. A sink that fails every batch forever (a poison envelope) has
  no dead-letter escape today — it simply redelivers indefinitely; a
  poison-envelope / dead-letter strategy is deliberately out of scope here
  and carried forward to phase 3.
- `rebuild_dispatch` is a library function today, exercised only by
  `tests/replay.rs` — there is no `koine-cli`/ops command wrapping it yet
  (phase 3's CLI is the natural home); running it against a live database is
  currently a by-hand operation, not a documented runbook.
