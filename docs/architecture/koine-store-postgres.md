# `koine-store-postgres`

## What it does

The production driven adapter: `PostgresEventStore` (`EventStore`),
`PostgresDispatcher` (`Dispatcher`), and `PostgresOutboxRelay` (delivers to an
`EventSink`), plus `rebuild_dispatch` (replay-from-zero ops tool) and
`connect_pool` (the one entry point composition roots use — connects and runs
the embedded `sqlx::migrate!` migrations). It is the durable twin of
`koine-store-memory`: the same two composite contracts (ADR 0011), a real
transaction standing in for the mutex guard.

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
  WHERE lease_id IS NOT NULL` (the sweep).
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
- **`extend_lease`/`expired` touch only `dispatch_queue`** — no event is
  written (ADR 0011-c). `extend_lease` sets `lease_expires_at` to a fresh
  `now + ttl` deadline (sliding window from the call, not from the previous
  deadline — see the phase-1b execution note below) and only if the lease is
  still live; `expired` lists jobs whose deadline has passed, feeding the
  sweep use case.
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
  (`dispatch_queue_rebuilds_identically_from_the_log`).
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

## Boundaries

- Depends on `koine-application` (implements its ports) and `koine-domain`
  (folds `Job`, emits `JobEvent`); no crate above it in the hexagon may bypass
  these ports to reach `sqlx` directly (ADR 0003).
- Requires Postgres — exercised at 11 (testcontainers-modules' default image,
  ring 3) through 17 (the `koine-server dev-loop` manual run); the schema's
  floor is native `GENERATED ALWAYS AS IDENTITY` columns (PG 10+).
- `koine-store-memory` is the behavioral twin: the crash-recovery lifecycle
  suite (`tests/lifecycle.rs`) mirrors `koine-store-memory`'s ring-2 story
  test-for-test against real SQL (ring 3).
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
