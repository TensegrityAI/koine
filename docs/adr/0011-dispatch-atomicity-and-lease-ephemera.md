# 0011 — Dispatch atomicity and lease ephemera

- **Status:** accepted
- **Date:** 2026-07-18
- **Context:** ADR 0006 makes the dispatch projection transactional with the
  event append. Phase 1 must fix *which component guarantees* the two
  composite operations that cannot be split: (a) append(events) + dispatch
  index update; (b) claim-eligible-job + append(JobLeased). And spec §3 makes
  heartbeats ephemeral — lease extension must not write events.
- **Decision:**
  - **(a) is the `EventStore::append` contract:** every adapter updates the
    dispatch index synchronously and atomically with the append, reacting to
    event kinds (enqueued/retry_scheduled → eligible; leased → claimed;
    succeeded/parked/cancelled/suspended → removed; lease_expired/failed →
    eligible now). The in-memory store implements this contract exactly as
    Postgres will (single transaction) so ring-2 tests exercise real
    semantics.
  - **(b) is the `Dispatcher::lease_next` contract:** the adapter atomically
    selects the highest-priority eligible job (priority DESC, then
    enqueue order; `not_before <= now`), produces `JobLeased` **via the
    domain aggregate** (domain validation stays authoritative — adapters may
    depend on domain), appends it, updates the index, and returns the
    `LeasedJob`. Use cases stay thin over this port; the orchestration
    atomicity lives where the transaction lives.
  - **(c) Lease extension is ephemeral:** `Dispatcher::extend_lease` updates the
    lease deadline in the dispatch index only. No event is written. Lease
    *expiry* is an event (`lease_expired`), produced by the sweep use case
    from `Dispatcher::expired`.
- **Consequences:** the dispatch index is rebuildable from the log (it is a
  projection), but is the only component allowed to hold ephemeral lease
  deadlines; adapters carry more responsibility and get contract tests;
  `Dispatcher` adapters need `IdGenerator`+`Clock` injected.
- **Alternatives considered:** two-phase claim-then-append in the use case
  (crash between = claimed-but-unrecorded limbo); `LeaseExtended` events
  (heartbeat-rate log spam, contradicts spec §3); merging Dispatcher into
  EventStore (one god-port, harder to keep honest).
