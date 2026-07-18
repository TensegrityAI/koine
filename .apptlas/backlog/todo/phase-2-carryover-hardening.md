# Close phase-1B recorded coverage/hygiene gaps before phase 2 lands

- **State:** todo
- **Origin:** phase 1B final review 2026-07-18
- **Epic:** none — cross-cutting hardening carried forward from phase 1B;
  touches phase 2 (`connect_pool` sizing under gRPC load) and flags a
  phase 3 concern (relay/sink shared-pool contention)

## Traceability

- **Implements:** closes recorded gaps from
  `.apptlas/backlog/done/retry-policy-ttl-bounds-hardening.md` (AC3's
  code-parity-not-test-parity note) and
  `.apptlas/backlog/done/phase-1b-postgres-store.md` (the unused-dependency
  and `rebuild_dispatch` ops-tool dispositions); no spec/ADR change, pure
  hardening.

## Acceptance criteria

- [ ] AC1: a ring-3 test drives `PostgresDispatcher::extend_lease` with a
  `Duration::MAX` ttl and asserts `DispatchError::Backend("ttl out of
  range")`, closing the recorded gap where Postgres-side TTL-overflow
  coverage was code-parity only (same guard as the memory store, never
  exercised against real Postgres) — *verify:* new test in
  `crates/koine-store-postgres/tests/dispatcher.rs`, named to mirror
  `koine-store-memory`'s `extend_lease_rejects_unrepresentable_ttl`.
- [ ] AC2: `tests/replay.rs`'s `dispatch_queue_rebuilds_identically_from_the_log`
  snapshot/resnapshot `SELECT`s extended from `(job_id, queue, priority,
  lease_id)` to also cover `not_before`, `worker_id`, and
  `lease_expires_at`, so the "rebuild lands on byte-identical rows" claim
  actually proves those columns match too, not just the four currently
  selected — *verify:* `cargo test -p koine-store-postgres --test replay
  dispatch_queue_rebuilds_identically_from_the_log`.
- [ ] AC3: a `cargo-machete` or `cargo-udeps` CI job catches unused
  *dependencies* (neither clippy nor `cargo deny` do today), and
  `koine-server`'s 5 declared-but-unreferenced deps (`koine-store-memory`,
  `koine-grpc`, `koine-http`, `koine-mcp`, `koine-observability`) are either
  pruned or wired to real code — *verify:* new CI job green; `cargo machete`
  (or `cargo udeps`) reports zero unused deps workspace-wide.
- [ ] AC4: `connect_pool` exposes a pool-size knob (today it calls
  `PgPool::connect(url)` with sqlx's silent default, no `max_connections`
  control), and the wiki records a phase-3 forward note that the outbox
  relay and any future `EventSink` sharing one pool with the hot
  dispatch/append path can deadlock under load if the pool is undersized
  (relay concurrency is still single-instance per ADR 0012, but the shared
  pool itself is a phase-3-relevant risk) — *verify:* `connect_pool` takes a
  configurable size (e.g. via `PgPoolOptions`) with a test asserting it's
  honored; note added to `docs/architecture/koine-store-postgres.md`'s
  Boundaries section.

## Dependencies

- None blocking — AC1/AC2 are test-only additions; AC3/AC4 touch
  `koine-server`'s manifest and `koine-store-postgres`'s `connect_pool`
  signature (a breaking change for any caller — currently only
  `koine-server`).

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
