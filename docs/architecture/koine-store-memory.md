# `koine-store-memory`

## What it does

Complete, non-stub implementations of `koine-application`'s driven ports —
`EventStore` (`InMemoryEventStore`) and `Dispatcher` (`InMemoryDispatcher`) —
plus deterministic `Clock`/`IdGenerator` test doubles. It exists to prove the
ports are adapter-neutral (ADR 0005) and to host the ring-2 lifecycle suite
that exercises use cases against real (if in-memory) atomicity.

## How it is built

- **One mutex is the "transaction" (`src/store.rs`)** — `InMemoryEventStore`
  holds an `Inner { streams, index, seq }` behind a single `Mutex`.
  `append_locked` validates (`expected_version` matches the stream's current
  length; every envelope continues it sequentially) *before* touching the
  map, so a rejected append leaves no phantom stream — a bug caught and
  fixed mid-implementation, now covered by
  `failed_append_leaves_no_phantom_stream`. Only on success does it extend
  the stream and re-fold it.
- **The dispatch index is a rebuildable projection** — `project_locked`
  re-derives a job's `DispatchEntry` from the *folded* `Job`, not from
  matching on the event variant just appended. `Pending` jobs get an entry
  keyed by queue/priority/`not_before`; `Leased`/`Running` jobs carry a
  `LeaseState`; every other state (terminal, or reserved phase-5) removes
  the entry. This means no adapter code binds to event-variant internals
  beyond the fold — the same shape the Postgres adapter will use with a real
  transaction instead of a mutex guard.
- **`InMemoryDispatcher` (`src/dispatcher.rs`)** — claims under the *same*
  store lock: `pick_eligible` selects highest-priority, then FIFO
  (`Reverse(seq)`), among unleased entries with `not_before <= now`, folds
  that job's stream, produces `leased` via `Job::lease` (domain stays
  authoritative per ADR 0011), and calls `InMemoryEventStore::append_locked`
  directly — claim and append share one lock acquisition, so there is no
  window where a job is claimed but not yet recorded. `extend_lease` and
  `expired` touch only the index's ephemeral `expires_at`; no event is
  written for either. `extend_lease` rejects an unrepresentable TTL as
  `DispatchError::Backend("ttl out of range")` rather than saturating —
  the same never-silently-clamp philosophy as the lease path, and matched
  by the Postgres twin's `extend_lease`.
- **Test doubles (`src/test_support.rs`)** — `FixedClock` (manually
  `advance`d) and `SeededIds` (sequential UUIDs seeding the high bits with a
  caller-chosen `seed`, and using that same `seed` as `jitter_seed()`) make
  every ring-2 test deterministic.
- **`NotifySignal`/`NoopPresence` (`src/signal.rs`, phase 2A)** — the
  in-memory `DispatchSignal`/`WorkerPresence` implementations `koine-grpc`'s
  test suites run against. `NotifySignal` holds one `tokio::sync::Notify`
  per queue (lazily created in a `Mutex<HashMap<QueueName, Arc<Notify>>>`);
  `notify` fetches-or-inserts the queue's `Notify` and calls
  `notify_waiters()`, `wait` races `notify.notified()` against the caller's
  `timeout`. Unlike `koine-store-postgres`'s `PgSignal`, nothing here is
  wired to `append` automatically — this store never calls
  `DispatchSignal::notify` itself on enqueue/retry; a caller that wants a
  memory-store-backed `Fetch` stream to wake promptly must call `notify`
  itself (the wire suite's `fetch_wakes_on_late_enqueue` does exactly this,
  with a comment explaining why). `NoopPresence` discards every `seen` call
  — the test-support presence double for suites that don't assert on
  presence rows.
- **Verification** — `src/store.rs` unit tests (append/load round-trip,
  version-conflict rejection, index maintenance, the phantom-stream
  regression), `src/dispatcher.rs` unit tests (priority/FIFO ordering,
  `not_before` gating, lease-plus-append atomicity, extend/expire), and
  `src/signal.rs` unit tests (`wait_returns_promptly_after_concurrent_notify_same_queue`,
  `wait_on_different_queue_times_out_at_timeout`,
  `noop_presence_seen_completes`). The crate-level proof is
  `tests/lifecycle.rs`: 8 tests running full use-case flows — happy path,
  retryable/non-retryable failure, cancel, crash recovery via the sweep,
  late-ack-after-expiry, heartbeat keep-alive, and exhaustion into
  `parked`.

## Why

- ADR 0005 — a complete in-memory adapter is what keeps `EventStore` (and,
  by extension, `Dispatcher`) honest as a port rather than secretly
  Postgres-shaped; it is also what lets application/use-case tests run fast
  with no Docker.
- ADR 0011 — this crate is the reference implementation of both composite
  contracts ((a) append + index update, (b) claim + append + index update)
  under one mutex, the same shape the Postgres adapter will deliver under
  one transaction.
- ADR 0013/0015 — `NotifySignal`/`NoopPresence` keep `DispatchSignal`/
  `WorkerPresence` adapter-neutral the same way the store/dispatcher pair
  keeps `EventStore`/`Dispatcher` honest.

## Boundaries

- Depends on `koine-application` (implements its ports) and `koine-domain`
  (folds `Job`, emits `JobEvent`).
- Test-oriented only — it is never a production store; the only durable
  backend is `koine-store-postgres`.
- Consumed by `koine-application`'s own tests, by
  `crates/koine-store-memory/tests/lifecycle.rs`, and (phase 2A) as a
  `[dev-dependencies]` of `koine-grpc`, whose `tests/wire.rs` and
  `tests/fetch_idle_disconnect.rs` build their `Deps` entirely on this
  crate's adapters (`InMemoryEventStore`, `InMemoryDispatcher`,
  `NotifySignal`, `NoopPresence`, `FixedClock`, `SeededIds`).
