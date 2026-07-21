# Close phase-1B recorded coverage/hygiene gaps before phase 2 lands

- **State:** done
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
- [x] AC4: `connect_pool` exposes a pool-size knob and the wiki records a
  phase-3 forward note that the outbox
  relay and any future `EventSink` sharing one pool with the hot
  dispatch/append path can deadlock under load if the pool is undersized
  (relay concurrency is still single-instance per ADR 0012, but the shared
  pool itself is a phase-3-relevant risk) — *verify:* `connect_pool` takes a
  configurable size (e.g. via `PgPoolOptions`) with a test asserting it's
  honored; note added to `docs/architecture/koine-store-postgres.md`'s
  Boundaries section.
  **Closed** (resource hardening, 2026-07-21): `PoolConfig` supplies non-zero
  pool size/acquisition-timeout values to `connect_pool`; ring-3
  `pool_options_are_honored` proves they reach SQLx. The Postgres wiki states
  the exact `N + 1` process budget and warns that phase-3 relay/sink
  concurrency requires a capacity review. The size-one, 32-waiter fan-out
  test proves the dedicated listener does not starve operational append work.

## Dependencies

- None blocking — AC1/AC2 are test-only additions; AC3 touches
  `koine-server`'s, `koine-store-memory`'s, `koine-store-postgres`'s, and
  `koine-proto`'s manifests; AC4 touches `koine-store-postgres`'s
  `connect_pool` signature (a breaking change for any caller — currently
  only `koine-server`).

## Evidence (closed, 2026-07-21)

AC1–AC3 remain closed by Task 8 (phase 2A branch, 2026-07-19); their
per-criterion commit and verification notes above remain the supporting
evidence. AC4 is closed by the reviewed resource-hardening slice: explicit
non-zero pool configuration is proven by `pool_options_are_honored`, the
Postgres architecture page documents the exact `N + 1` budget and phase-3
relay/sink capacity review, and the 32-waiter size-one pressure test proves
the listener leaves operational append capacity available. Final `make ci`
and `git diff --check` pass. The current workspace test inventory is 127;
the historical 126-test base-slice run is not closure evidence.

## Independent review verdict (2026-07-21)

- Spec compliance: ✅ Faithful to hardening design §5 and legacy carryover AC4.
- Quality: Approved — no Critical, Important, or unrecorded Minor findings.

## Spec-fidelity statement

Faithful to the recorded phase-1B carryover AC1–AC4. No specification or ADR
change was required; this is the documented, reviewed hardening closure.
