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

- [x] AC1: a ring-3 test drives `PostgresDispatcher::extend_lease` with a
  `Duration::MAX` ttl and asserts `DispatchError::Backend("ttl out of
  range")`, closing the recorded gap where Postgres-side TTL-overflow
  coverage was code-parity only (same guard as the memory store, never
  exercised against real Postgres) — *verify:* new test in
  `crates/koine-store-postgres/tests/dispatcher.rs`, named to mirror
  `koine-store-memory`'s `extend_lease_rejects_unrepresentable_ttl`.
  **Closed** (Task 8, 2026-07-19): `extend_lease_rejects_unrepresentable_ttl`
  added, green on first run (1B's `chrono::TimeDelta::from_std` guard
  already covers Postgres too); mutation-checked by hand (guard temporarily
  disabled, test failed, reverted) to confirm it is load-bearing, not
  vacuous. Commit `1b17451`.
- [x] AC2: `tests/replay.rs`'s `dispatch_queue_rebuilds_identically_from_the_log`
  snapshot/resnapshot `SELECT`s extended from `(job_id, queue, priority,
  lease_id)` to also cover `not_before`, `worker_id`, and
  `lease_expires_at`, so the "rebuild lands on byte-identical rows" claim
  actually proves those columns match too, not just the four currently
  selected — *verify:* `cargo test -p koine-store-postgres --test replay
  dispatch_queue_rebuilds_identically_from_the_log`.
  **Closed** (Task 8, 2026-07-19): SELECT and `DispatchRow` tuple both grew
  to 7 columns (job_id, queue, priority, lease_id, not_before, worker_id,
  lease_expires_at); assertion text unchanged; test green. Commit `ecb1f5b`.
- [x] AC3: a `cargo-machete` or `cargo-udeps` CI job catches unused
  *dependencies* (neither clippy nor `cargo deny` do today), and
  `koine-server`'s 5 declared-but-unreferenced deps (`koine-store-memory`,
  `koine-grpc`, `koine-http`, `koine-mcp`, `koine-observability`) are either
  pruned or wired to real code — *verify:* new CI job green; `cargo machete`
  (or `cargo udeps`) reports zero unused deps workspace-wide.
  **Closed** (Task 8, 2026-07-19): `unused-deps` CI job + local `make
  machete` (wired into `make ci`) added. `koine-grpc` is now genuinely used
  (Task 7's `serve.rs`); `koine-store-memory`, `koine-http`, `koine-mcp`,
  `koine-observability` pruned from `koine-server`. Getting the job green
  workspace-wide (per this AC's own verify text) also required pruning
  genuinely-unused `thiserror` from `koine-store-memory`/
  `koine-store-postgres` and adding a documented
  `[package.metadata.cargo-machete]` `ignored` entry to `koine-proto` for
  `prost`/`tonic-prost` (used only via `tonic::include_proto!`'s
  compile-time `include!()`, invisible to machete's static scan) —
  broader than this AC's literal koine-server-only text, disclosed here.
  `cargo machete` reports zero unused deps workspace-wide. Commit `0093d07`.
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
  **→ 2B/3** — out of Task 8's 2A-scoped cut; the pool-knob change and its
  relay/sink shared-pool deadlock note stay open for whichever of phase 2B
  or 3 first adds a second consumer of `connect_pool`'s pool.

## Dependencies

- None blocking — AC1/AC2 are test-only additions; AC3 touches
  `koine-server`'s, `koine-store-memory`'s, `koine-store-postgres`'s, and
  `koine-proto`'s manifests; AC4 touches `koine-store-postgres`'s
  `connect_pool` signature (a breaking change for any caller — currently
  only `koine-server`).

## Evidence (filled at close)

Partial — AC1–AC3 closed by Task 8 (phase 2A branch, 2026-07-19); see
per-AC **Closed** notes above for commits and verification. AC4 remains
open, so this item stays in `todo/` rather than moving to `done/` (task
lifecycle: a `todo/`→`done/` move requires every DoD point to hold). Full
`make ci` (now including the new `unused-deps`/`make machete` step) green
throughout; all 20 `koine-store-postgres` ring-3 tests green against a real
Postgres container (Docker/testcontainers).

2026-07-21 pre-review resource-hardening evidence for AC4: `PoolConfig`
passes the non-zero `KOINE_DB_MAX_CONNECTIONS` and
`KOINE_DB_ACQUIRE_TIMEOUT_MS` settings to `connect_pool`; ring-3
`pool_options_are_honored` passes against Postgres. The architecture wiki now
states the exact `N + 1` budget and phase-3 relay/sink capacity-review warning.
The shared-listener pressure test also passes with one operational connection,
32 idle waits, and one dedicated listener, leaving the operational connection
available for append. This functional evidence is intentionally not an AC4
closure: independent Step 3 verdicts and the resource item's final lifecycle
work are still pending, so AC4 remains unchecked and the `→ 2B/3` disposition
remains in force.

## Spec-fidelity statement (filled at close)

Deferred until AC4 is independently reviewed and this item moves to `done/`.
