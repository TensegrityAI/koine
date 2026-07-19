# Phase 2A — Data-plane server (TLA+ model, koine.v1 wire contract, gRPC)

- **State:** done
- **Origin:** plan `docs/superpowers/plans/2026-07-19-koine-phase-2a-data-plane-server.md`
- **Epic:** `../epics/phase-2-data-plane.md` (items 1–6)

## Traceability

- **Implements:** design spec §2 (data plane, contract), §3 (delivery);
  ADRs 0013 (wire contract v1), 0014 (worker auth v1), 0015 (worker
  presence as ephemeral state); the phase-1 epic's `WorkerRegistration`
  disposition (resolved by ADR 0015, not the event-sourced aggregate the
  spec originally sketched).

## Acceptance criteria

- [x] AC1: the TLA+ lease/delivery-protocol model is TLC-checked — 7
  invariants (`TypeOK`, `NoDualLease`, `FreshLeases`, `AttemptCapped`,
  `LeaseFencingOK`, `NoLeaseWhenIdle`, `NonRetryableAlwaysParks`) plus the
  `EventuallySettled` liveness property, over every reachable state, in CI
  (the `tla` job, `.github/workflows/ci.yml`, `actions/setup-java@v4`
  Temurin 21) and locally via `make tla` — *verify:* `make tla` → "Model
  checking completed. No error has been found." (611 states generated, 184
  distinct, search depth 12, re-run at this closeout); CI run id: **pending
  merge CI run** (this branch's own `tla` job has not yet run against the
  merged main branch at closeout time).
- [x] AC2: the `koine.v1` wire contract compiles and is additive-only by
  policy — *verify:* `cargo build -p koine-proto`; `proto/koine/v1/worker.proto`
  documents the additive-only/`reserved` policy in its header comment; ADR
  0013 records the RPC shape and evolution rule.
- [x] AC3: the in-process wire suite proves the real tonic transport (not
  just direct trait calls) against every RPC, including auth rejection and
  signal-driven wakeup — *verify:* `cargo test -p koine-grpc --test wire`
  (6 tests: `unauthenticated_calls_are_rejected`,
  `fetch_streams_a_claimed_job`, `full_story_over_the_wire`,
  `stale_ack_returns_conflict`, `fetch_wakes_on_late_enqueue`,
  `heartbeat_reports_liveness`).
- [x] AC4: a fetch stream wakes on new work via the dispatch signal, not
  the idle-poll fallback — *verify:* `fetch_wakes_on_late_enqueue` (test 5
  of AC3's suite) uses a 10s `idle_poll` and asserts the job arrives within
  1s, so any observed wakeup faster than the fallback ceiling can only have
  come from `DispatchSignal`, not the poll; ring-3 twin:
  `cargo test -p koine-store-postgres --test signal
  signal_wait_wakes_on_append_to_queue` (real Postgres `LISTEN`/`NOTIFY`).
- [x] AC5: a dropped `Fetch` stream does not leak the spawned polling task
  — *verify:* `cargo test -p koine-grpc --test fetch_idle_disconnect`
  (`fetch_task_ends_when_receiver_drops_while_idle`) — a review-round fix
  (`tokio::select!` racing `tx.closed()` against `signal.wait`), regression-
  tested here.
- [x] AC6: auth is enforced on every RPC, with constant-time comparison and
  no detail leakage on failure — *verify:* `cargo test -p koine-grpc
  --lib` (`src/auth.rs`, 10 unit tests: `valid_pair_is_ok`,
  `wrong_token_same_length_is_unauthenticated`,
  `wrong_token_different_length_is_unauthenticated`,
  `missing_authorization_header_is_unauthenticated`,
  `missing_worker_id_header_is_unauthenticated`,
  `empty_worker_id_is_unauthenticated`,
  `control_char_worker_id_is_unauthenticated`,
  `missing_bearer_prefix_is_unauthenticated`,
  `empty_expected_token_rejects_empty_presented_token`,
  `empty_expected_token_rejects_any_presented_token`) plus the wire-level
  `unauthenticated_calls_are_rejected` (AC3) and `koine-server`'s
  `missing_token_is_refused`/`empty_token_is_refused` (env-level refusal to
  start unauthenticated, `crates/koine-server/src/serve.rs`).
- [x] AC7: worker presence rows are recorded on authenticated data-plane
  calls, surviving as durable state — *verify:*
  `cargo test -p koine-store-postgres --test signal
  presence_records_worker_with_queue` (upsert semantics: `last_seen`
  advances, `last_queue` preserved via `COALESCE` when a later call omits
  it) and `cargo test -p koine-grpc --test grpc_e2e presence_rows_appear`
  (proves `fetch` alone — no job needed — registers the worker, over a
  real server + real Postgres).
- [x] AC8: the crash-recovery story is proven end-to-end over a real
  socket and a real store, not only in-process — *verify:* `cargo test -p
  koine-grpc --test grpc_e2e crash_recovery_over_the_wire` — real TCP
  loopback + real Postgres; exact arc `enqueued → leased → lease_expired →
  retry_scheduled → leased → started → succeeded`, plus the crashed
  worker's late `succeed` recorded as `late_ack_conflict`, never dropped
  and never a transport error.
- [x] AC9: phase-2-carryover-hardening's 2A-scoped ACs are closed — *verify:*
  `.apptlas/backlog/todo/phase-2-carryover-hardening.md` AC1
  (`extend_lease_rejects_unrepresentable_ttl`, commit `1b17451`), AC2
  (`dispatch_queue_rebuilds_identically_from_the_log` extended to 7
  columns, commit `ecb1f5b`), AC3 (`unused-deps` CI job + `make machete`,
  commit `0093d07`) — all three closed on this branch; AC4 (pool-size knob)
  stays open, moved to 2B/3 per its own text, the item itself remains in
  `todo/`.

## Dependencies

- Phase 1 (event-sourced core, ports, memory + Postgres stores) — complete.
- ADRs 0013/0014/0015 — accepted before implementation began (Task 1).

## Evidence (filled at close)

**Test suite — 108 tests total, all green** (`cargo test --workspace`,
reran at this closeout):

- Ring 1 (`koine-domain`): 33 unit + 3 property = 36 (unchanged this phase).
- Ring 2 (`koine-application`: 1; `koine-store-memory`: 15 unit — the 12
  from phase 1B plus 3 new in `src/signal.rs`
  (`wait_returns_promptly_after_concurrent_notify_same_queue`,
  `wait_on_different_queue_times_out_at_timeout`,
  `noop_presence_seen_completes`) — + 10 lifecycle) = 26.
- Ring 3 (`koine-store-postgres`, real Postgres via testcontainers, real
  migrations): 5 (`store.rs`) + 4 (`dispatcher.rs`, +1 this phase:
  `extend_lease_rejects_unrepresentable_ttl`) + 2 (`outbox.rs`) + 5
  (`lifecycle.rs`) + 1 (`replay.rs`, extended in place to 7 snapshot
  columns) + 3 (`signal.rs`, new this phase:
  `signal_wait_wakes_on_append_to_queue`,
  `signal_wait_on_other_queue_times_out`,
  `presence_records_worker_with_queue`) = 20.
- `koine-grpc` (new crate, real tonic transport): 10 unit (`src/auth.rs`) +
  6 (`tests/wire.rs`, in-process duplex + in-memory adapters) + 1
  (`tests/fetch_idle_disconnect.rs`) + 2 (`tests/grpc_e2e.rs`, real TCP +
  real Postgres) = 19.
- `koine-server`: 7 (`src/serve.rs`'s `parse_config` unit tests:
  `missing_token_is_refused`, `empty_token_is_refused`,
  `defaults_apply_when_only_token_is_set`, `overrides_are_parsed`,
  `invalid_addr_is_rejected`, `invalid_ttl_is_rejected`,
  `invalid_idle_poll_is_rejected`); `dev_loop.rs`'s own acceptance check
  remains the `dev-loop` product run, not a `#[test]`.
- `koine-proto`/`koine-cli`/`koine-http`/`koine-mcp`/`koine-observability`:
  0 tests each — `koine-proto` is a generated-code contract crate proven by
  its consumers' tests; the other four remain documented stubs.
- Total: 36 + 26 + 20 + 19 + 7 = **108, 0 failed** (up from phase 1B's 75:
  +33 from `koine-grpc`'s new suites, +3 memory-store signal tests, +1
  Postgres dispatcher test (carryover AC1), +7 `koine-server` config tests;
  `replay.rs`'s carryover AC2 change extended an existing test in place, no
  count change).

**Gate:** `make ci` → fmt-check, clippy `-D warnings` (workspace, all
targets), `cargo test --workspace`, `cargo doc -D warnings`, `cargo deny
check`, `typos`, markdownlint, `make machete` (`cargo machete`, the
carryover AC3 addition) — all green, including this task's new/updated
wiki pages. `make tla` green separately (Java/TLC toolchain, not part of
`make ci`): "Model checking completed. No error has been found." (611
states generated, 184 distinct, search depth 12).

**AC1 — TLC evidence (reran at this closeout):**

```text
$ make tla
...
Model checking completed. No error has been found.
611 states generated, 184 distinct states found, 0 states left on queue.
The depth of the complete state graph search is 12.
```

CI's own `tla` job on this branch's final commit has not yet produced a run
id at the time of this writing — **pending merge CI run**; the identical
`make tla` invocation is what that job runs, so the local result above is
the same check.

**AC8 — crash-recovery-over-the-wire arc (loaded back from real Postgres,
`crash_recovery_over_the_wire`):**

```text
enqueued, leased, lease_expired, retry_scheduled, leased, started,
succeeded, late_ack_conflict
```

Worker A fetches (attempt 0), "crashes" (drops the stream, never acks); the
sweep reclaims the expired lease; worker B fetches the same job (attempt
1), starts, succeeds; worker A's late `succeed` against its now-stale lease
comes back `ACK_OUTCOME_CONFLICT` — recorded, never a transport error.

## Spec-fidelity statement (filled at close)

Faithful to spec §2/§3 and ADRs 0013–0015, with recorded dispositions and
the honest execution history below.

**Recorded dispositions (ADR'd divergences):**

- **Server-streaming `Fetch` + unary acks, not the spec's bidi-stream
  diagram** (ADR 0013): a full bidi protocol multiplexing acks into the
  stream would add session-state complexity v1 doesn't need; revisit is
  tied to phase-2B benchmarks. `overview.md`'s architecture diagram is
  updated this closeout to show the delivered shape rather than the
  original bidi sketch, with a note explaining the divergence in place.
- **Worker presence is ephemeral infrastructure state, not an event-sourced
  `WorkerRegistration` aggregate** (ADR 0015): a `workers` table upserted on
  every authenticated call, no domain events, no stream; survives restarts
  as rows filtered by `last_seen`. No audit history of fleet churn — revisit
  if a real consumer needs it.
- **TLS is proxy-terminated, not native** (ADR 0014): the server binds
  plain HTTP/2; deployment guidance is a TLS-terminating ingress in front.
  Documented honestly in both ADR 0014 and this closeout's wiki pages —
  never claimed as more.
- **Auth is a plain function called per-handler, not a tonic `Interceptor`
  layer**: `auth::check(metadata, token)` is the first line of every RPC
  method in `service.rs`. Functionally equivalent to the plan's informal
  "bearer-token interceptor" framing, but worth stating precisely: there is
  no tower/tonic middleware wrapping the service, and the wiki pages say so
  rather than imply a layered interceptor exists.
- **No checkpoint RPC**: `koine-grpc`'s and `koine-proto`'s crate-level doc
  comments mention "checkpoints", inherited unedited from an earlier
  scoping pass; no checkpoint RPC exists in `worker.proto` or anywhere in
  either crate. Flagged in both new wiki pages rather than silently
  reflected as delivered.
- **Carryover AC4 (pool-size knob) not closed this phase** — out of 2A's
  scope per the carryover item's own text; `phase-2-carryover-hardening.md`
  stays in `todo/` with AC4 open, moved to 2B/3.
- Epic item 9 (scripted crash-recovery demo) is only partially represented:
  `crash_recovery_over_the_wire` proves the exact arc as an automated test
  over a real socket and real Postgres, but the epic's original "kill the
  worker mid-job" framing implied a demo against a real SDK — no SDK exists
  yet (item 7, phase 2B), so the product-level scripted demo itself is
  carried forward, not delivered here.
- Epic item 12 (wiki pages) did not produce separate `data-plane.md`/
  `formal-models.md` pages as originally listed: `docs/formal/README.md`
  already covers the formal-model content current from Tasks 1–2, and the
  per-crate pages (`koine-proto.md`, `koine-grpc.md`) plus `overview.md`'s
  updated data-plane section cover what a `data-plane.md` would otherwise
  restate. Judged redundant rather than an oversight; noted here for the
  record.

**Execution deviations (found and corrected before close, the honest
record):**

- **TLA+ invariant set strengthened beyond the plan's original four safety
  properties** (fix round, commit `28f29d7`): the plan's skeleton named
  `TypeOK`/`NoDualLease`/`FreshLeases`/`AttemptCapped`. Mutation-probe
  testing (documented in `docs/formal/README.md`) found these four pass
  even under two protocol-breaking mutations (a weakened `AckSucceed`
  guard, and `Expire` failing to clear `activeLease`) — neither mutation
  touches a variable those four inspect. `LeaseFencingOK`, `NoLeaseWhenIdle`,
  and `NonRetryableAlwaysParks` were added specifically to close that gap,
  using ghost variables to make transition-relative facts (not just
  post-transition state) checkable; all three are probe-verified to catch
  their respective mutations.
- **`lease_protocol.cfg` needed a `StateConstraint` and `CHECK_DEADLOCK
  FALSE` to terminate cleanly** — `conflicts` is unbounded in the module
  (`LateAck` increments it with no guard), so raw exploration never
  terminates; `MaxConflicts = 3` bounds it, re-verified at 1 and 6 to
  confirm the bound doesn't hide anything. `CHECK_DEADLOCK FALSE` tells TLC
  that `succeeded`/`cancelled` having no enabled next action is expected,
  not a bug. Neither changes the modeled protocol semantics — both are
  `.cfg`-only.
- **Empty-token auth guard added in review** (`fix(grpc): reject empty auth
  token and end fetch task on client drop`, commit `051ba6c`): an initial
  implementation's length-then-`ct_eq` comparison would treat an empty
  configured token and an empty presented token as `0 == 0`, authenticating
  everyone. Fixed by rejecting outright whenever the configured token is
  empty, before any comparison runs — closes a real "auth silently
  disabled" hole, not a hypothetical one.
- **Fetch-task leak fixed in the same commit** (`051ba6c`): the original
  idle-poll arm only awaited `deps.signal.wait(...)`; a client dropping the
  stream while the queue was idle left the spawned task polling forever,
  never observing the closed receiver. Fixed by racing `tokio::select!`
  against `tx.closed()`; regression-tested by
  `fetch_task_ends_when_receiver_drops_while_idle`.
- **`PgSignal::wait`'s timeout widened to the whole operation, not just
  `recv()`** (commit `2180acd`, `fix(store): honor signal wait timeout`): an
  earlier version wrapped only the `listener.recv()` calls in
  `tokio::time::timeout`, so a slow pool acquire or `LISTEN` setup could
  make a caller wait past its stated budget. Fixed by wrapping the entire
  connect+listen+recv-loop in one outer timeout.
- **`cargo-machete` scope extended beyond `koine-server`'s manifest**
  (commit `0093d07`, carryover AC3): getting the workspace-wide gate green
  also required pruning genuinely-unused `thiserror` from
  `koine-store-memory`/`koine-store-postgres` and adding a documented
  `[package.metadata.cargo-machete]` ignore entry to `koine-proto` for
  `prost`/`tonic-prost` (used only via `tonic::include_proto!`'s
  compile-time `include!()`, invisible to machete's static scan) — broader
  than AC3's literal text, disclosed in that backlog item and here.
- **Zero-jitter retry policy needed for a deterministic e2e redelivery
  wait** (`crates/koine-grpc/tests/grpc_e2e.rs::instant_retry_policy`): the
  domain's default retry policy's jitter would put `retry_scheduled`'s
  `not_before` anywhere up to 2s in the future against real wall-clock time
  (the e2e suite can't fast-forward a `FixedClock` the way ring-2/3 unit
  tests do), turning the crash-recovery test's redelivery wait into a
  flaky, non-deterministic sleep. `instant_retry_policy` zeroes
  `base_delay` so `not_before == now`, keeping the test both real (wall
  clock, real Postgres) and fast.
- **`SystemClock`/`UuidV7Ids` duplicated into `koine-grpc`'s and
  `koine-store-postgres`'s test-support modules**: both are verbatim copies
  of `koine-server/src/runtime.rs`'s production types, because
  `koine-server` is a bin-only crate and so cannot be a dev-dependency of
  either. Both copies carry a comment naming the phase-2B dedup follow-up
  (a shared `tests/support` or test-fixtures crate) rather than presenting
  the duplication as unnoticed.

No spec statement is contradicted; every divergence above is either an
ADR-recorded decision, a disposition recorded at the point it was made, or
a bug caught and corrected before this item closed.
