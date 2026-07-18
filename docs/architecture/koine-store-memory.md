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
  written for either.
- **Test doubles (`src/test_support.rs`)** — `FixedClock` (manually
  `advance`d) and `SeededIds` (sequential UUIDs seeding the high bits with a
  caller-chosen `seed`, and using that same `seed` as `jitter_seed()`) make
  every ring-2 test deterministic.
- **Verification** — `src/store.rs` unit tests (append/load round-trip,
  version-conflict rejection, index maintenance, the phantom-stream
  regression) and `src/dispatcher.rs` unit tests (priority/FIFO ordering,
  `not_before` gating, lease-plus-append atomicity, extend/expire). The
  crate-level proof is `tests/lifecycle.rs`: 8 tests running full use-case
  flows — happy path, retryable/non-retryable failure, cancel, crash
  recovery via the sweep, late-ack-after-expiry, heartbeat keep-alive, and
  exhaustion into `parked`.

## Why

- ADR 0005 — a complete in-memory adapter is what keeps `EventStore` (and,
  by extension, `Dispatcher`) honest as a port rather than secretly
  Postgres-shaped; it is also what lets application/use-case tests run fast
  with no Docker.
- ADR 0011 — this crate is the reference implementation of both composite
  contracts ((a) append + index update, (b) claim + append + index update)
  under one mutex, the same shape the Postgres adapter will deliver under
  one transaction.

## Boundaries

- Depends on `koine-application` (implements its ports) and `koine-domain`
  (folds `Job`, emits `JobEvent`).
- Test-oriented only — it is never a production store; the only durable
  backend is `koine-store-postgres` (phase 1B).
- Consumed today by `koine-application`'s own tests and by
  `crates/koine-store-memory/tests/lifecycle.rs`; phase 1B's Postgres
  contract tests will assert the same behavior against the real adapter.
