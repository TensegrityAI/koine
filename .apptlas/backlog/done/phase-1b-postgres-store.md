# Phase 1B — Postgres store, outbox, ring 3, dev-loop

- **State:** done
- **Origin:** plan docs/superpowers/plans/2026-07-18-koine-phase-1b-postgres-store.md
- **Epic:** ../epics/phase-1-event-sourced-core.md (items 8–11)

## Traceability

- **Implements:** design spec §3 hot path (dispatch projection synchronous
  with append, everything else via outbox); ADR 0012 (Postgres store
  mechanics); ADRs 0005/0006/0011 made concrete in the production adapter.

## Acceptance criteria

- [x] AC1: ring-3 contract parity with the memory store — append/load
  round-trip, version-conflict rejection, and side-effect-free failed
  appends against BOTH a fresh stream and an existing one (prior events
  survive) — *verify:* `cargo test -p koine-store-postgres --test store`
  (`appends_and_loads_round_trip`, `rejects_version_conflicts`,
  `failed_append_leaves_no_trace_fresh_or_existing`,
  `append_maintains_dispatch_row_and_outbox`, `migrations_apply_cleanly`)
- [x] AC2: `SELECT … FOR UPDATE SKIP LOCKED` claim never double-claims —
  priority/FIFO ordering, `not_before` gating, and concurrent claimers land
  distinct jobs — *verify:* `cargo test -p koine-store-postgres --test
  dispatcher` (`claims_by_priority_then_fifo_and_appends_leased`,
  `respects_not_before_and_lease_expiry`,
  `concurrent_claims_get_distinct_jobs`)
- [x] AC3: outbox relay delivers in `outbox_seq` order and a sink failure
  rolls the claim back for redelivery rather than losing or reordering the
  batch — *verify:* `cargo test -p koine-store-postgres --test outbox`
  (`relays_in_order_and_deletes_on_success`,
  `sink_failure_rolls_back_for_redelivery`)
- [x] AC4: the ring-2 crash-recovery lifecycle story (worker crash → sweep
  recovery, late-ack-after-expiry conflict recording, retry exhaustion into
  `parked`, non-transition sweep faults surfacing) mirrors test-for-test
  against real Postgres — *verify:* `cargo test -p koine-store-postgres
  --test lifecycle` (`happy_path_records_the_full_story`,
  `worker_crash_is_recovered_by_the_sweep`,
  `late_ack_after_expiry_is_recorded_never_lost`,
  `repeated_crashes_exhaust_into_parked`,
  `sweep_surfaces_non_transition_domain_errors`)
- [x] AC5: `rebuild_dispatch` replays the event log into a `dispatch_queue`
  identical to the one incremental projection produced — the epic's "every
  projection replays from event zero to an identical state" exit criterion,
  made executable — *verify:* `cargo test -p koine-store-postgres --test
  replay` (`dispatch_queue_rebuilds_identically_from_the_log`)
- [x] AC6: the full stack — enqueue, worker, sweep, outbox relay — runs
  end-to-end against a real Postgres as a product exercise, not only through
  tests (DoD item 2) — *verify:* `DATABASE_URL=... cargo run -p koine-server
  -- dev-loop` against `docker run postgres:17`; captured story pasted below

## Dependencies

- Phase 1A (domain core, rings 1–2) — complete.
- Hardening item `retry-policy-ttl-bounds-hardening` (closed alongside this
  item; its AC5 contract test is inherited verbatim by AC1 above).

## Evidence (filled at close)

**Test suite — 75 tests total, all green** (`cargo test --workspace`, reran
at this closeout):

- Ring 1 (`koine-domain`): 33 unit + 3 property = 36.
- Ring 2 (`koine-application`: 1; `koine-store-memory`: 12 unit + 10
  lifecycle, the 10 including this phase's two hardening additions
  `enqueue_rejects_pathological_retry_policies` and
  `sweep_surfaces_non_transition_domain_errors`) = 23.
- Ring 3 (`koine-store-postgres`, real Postgres via testcontainers, real
  migrations, 0 unit tests): 5 (`store.rs`) + 3 (`dispatcher.rs`) + 2
  (`outbox.rs`) + 5 (`lifecycle.rs`) + 1 (`replay.rs`) = 16.
- `koine-cli`/`koine-grpc`/`koine-http`/`koine-mcp`/`koine-observability`/
  `koine-proto`/`koine-server`: 0 tests each — still documented stubs or (for
  `koine-server`) a binary composition root whose dev-loop run is the
  acceptance test (see AC6 below).
- Total: 36 (ring 1) + 23 (ring 2) + 16 (ring 3) = **75, 0 failed**.

**Gate:** `make ci` → fmt-check, clippy `-D warnings` (workspace, all
targets), `cargo test --workspace`, `cargo doc -D warnings`, `cargo deny
check`, `typos`, markdownlint — all green, including this task's new/updated
pages.

**AC6 — dev-loop product exercise (captured output, `koine-server dev-loop`
against `docker run -d postgres:17`, exit 0):**

```text
$ docker run -d --name koine-devloop -e POSTGRES_PASSWORD=koine -e POSTGRES_USER=koine \
    -e POSTGRES_DB=koine -p 55432:5432 postgres:17
$ DATABASE_URL=postgres://koine:koine@localhost:55432/koine cargo run -p koine-server -- dev-loop

   Compiling koine-server v0.1.0 (/home/nexus/workspaces/nexus/crates/koine-server)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.32s
     Running `/home/nexus/.cache/cargo-target/debug/koine-server dev-loop`
dev-loop: enqueued job1(plain)=019f7716-6d65-77f0-a73a-2c8909570a06 job2(crashy)=019f7716-6da3-7990-8713-a89786c6913d job3(flaky)=019f7716-6dac-7322-8052-4adc9601e6f0
  [outbox→sink] 019f7716-6d65-77f0-a73a-2c8909570a06 v1 enqueued
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v1 enqueued
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v1 enqueued
  [outbox→sink] 019f7716-6d65-77f0-a73a-2c8909570a06 v2 leased
  [outbox→sink] 019f7716-6d65-77f0-a73a-2c8909570a06 v3 started
  [outbox→sink] 019f7716-6d65-77f0-a73a-2c8909570a06 v4 succeeded
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v2 leased
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v2 leased
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v3 started
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v4 failed
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v5 retry_scheduled
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v6 leased
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v7 started
  [outbox→sink] 019f7716-6dac-7322-8052-4adc9601e6f0 v8 succeeded
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v3 lease_expired
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v4 retry_scheduled
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v5 leased
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v6 started
  [outbox→sink] 019f7716-6da3-7990-8713-a89786c6913d v7 succeeded
dev-loop: job1 (plain) story:  enqueued,leased,started,succeeded
dev-loop: job2 (crashy) story: enqueued,leased,lease_expired,retry_scheduled,leased,started,succeeded
dev-loop: job3 (flaky) story:  enqueued,leased,started,failed,retry_scheduled,leased,started,succeeded
dev-loop: all jobs terminal — stack exercised end-to-end

$ echo "exit code: $?"
exit code: 0
$ docker rm -f koine-devloop
koine-devloop
```

job1's story is the exact plain happy path; job2 shows the simulated crash
(`lease_expired` → `retry_scheduled`, recovered only by the sweep) then a
clean second attempt; job3 shows a retryable failure (`failed` →
`retry_scheduled`) then success on attempt 1. All three self-verifying story
assertions in `check_stories` passed (no missing-marker `Err`). Full
narrative and deviations in `.superpowers/sdd/task-9-report.md`.

## Spec-fidelity statement (filled at close)

Faithful to spec §3 and ADR 0012, with recorded dispositions and the honest
execution history below.

**Recorded dispositions:**

- The outbox relay is single-instance by ADR 0012's own decision (identity
  sequences interleave under concurrency, so claim-delete instead of
  position-tracking is correct only with one relay); consumer positions and
  relay concurrency are deferred to phase 3's real read projections
  (disposition: recorded here, ADR 0012).
- A sink that always errors has no dead-letter/poison-envelope escape — it
  redelivers forever. Deliberately out of scope for 1B; carried forward as a
  phase-3 design question alongside consumer positions (disposition: forward
  note, not a defect — no v1 sink is expected to fail permanently).
- `rebuild_dispatch` is a library function proven by `tests/replay.rs`, not
  yet wrapped in an operator-facing command; running it against a live
  database today is a by-hand operation. Phase 3's `koine-cli` is the natural
  home for a wrapping runbook (disposition: forward note).
- `koine-server`'s `Cargo.toml` still declares `koine-store-memory`,
  `koine-grpc`, `koine-http`, `koine-mcp`, `koine-observability` as
  dependencies with no `use` referencing them (inherited from the phase-0
  scaffold, verified rather than pruned per that task's own brief). Neither
  `cargo deny` nor clippy flag unused *dependencies* (only unused imports),
  so this asymmetry is real but currently invisible to CI (disposition:
  forward note — a `cargo-udeps`-style CI check is a candidate for a future
  hardening item, not filed as one yet).

**Execution deviations (found and corrected before close, the honest
record):**

- **`extend_lease` accumulating-semantics deviation caught and reverted**
  (commit `6a1a3fe`): an intermediate version of
  `PostgresDispatcher::extend_lease` issued `SET lease_expires_at =
  lease_expires_at + $1` — extending relative to the *current* deadline
  (accumulating) instead of `now + ttl` (sliding window from the call), the
  semantics the memory store implements and `extend_lease_rejects_
  unrepresentable_ttl`'s contract assumes. The bug went unnoticed initially
  because `tests/dispatcher.rs::respects_not_before_and_lease_expiry` reused
  the same 30s `ttl` for both the initial claim and the heartbeat extension —
  accumulating 30s onto a deadline already 30s out landed on the same
  absolute instant sliding-by-60s would, so the test could not distinguish
  the two semantics. Caught in review; fixed by changing the SQL to
  `lease_expires_at = $1` with `$1` bound to a freshly computed `now + delta`
  deadline, and — because the defect was in the *test's* power to detect the
  bug, not only the implementation — by changing the test's heartbeat call to
  `Duration::from_mins(1)` (a different duration from the claim ttl), which
  now fails if the accumulating behavior ever regresses.
- **`deny.toml` advisory-suppression replaced by a real fix** (commits
  `91cd1d9` → `d6768d5` → `17d8c3f`): standing up the ring-3 harness first
  landed with `testcontainers 0.26`/`testcontainers-modules 0.14`, whose
  transitive deps tripped five RUSTSEC advisories; the first pass suppressed
  them with an `[advisories.ignore]` list (comments attached, but still a
  suppression). Corrected before close: bumped to `testcontainers 0.27`/
  `testcontainers-modules 0.15`, which eliminates all five advisories
  outright, removed the ignore list entirely, and committed the regenerated
  lockfile — `cargo deny check` is clean with no ignored advisories.
- **`checked_add_signed` domain fix** (commit `d63ad73`): `Job`'s retry-decision
  path computed `now + delay` directly on a `chrono::DateTime`, which panics
  on overflow rather than erroring; found while adding the hardening item's
  enqueue-bounds validation (which closes the client-supplied side of this
  gap) and fixed at the domain layer too, for any delay that still reaches
  this line: `now.checked_add_signed(delay).ok_or(DomainError::InvalidTtl)?`
  replaces the panicking `now + delay`, so an unrepresentable deadline is a
  typed `InvalidTtl` error instead of a process crash. Covered by
  `sweep_surfaces_non_transition_domain_errors` in both `koine-store-memory`
  and `koine-store-postgres`.
- Final whole-branch review (2026-07-18): stale wiki pages for
  koine-application/domain/store-memory (1B hardening surface) found and
  fixed pre-merge — plan scoping gap recorded; rebuild_dispatch
  quiesced-writers warning added (concurrent-use lease-overwrite hazard).

No spec statement is contradicted; every divergence above is either an
ADR-recorded deferral or a bug caught and corrected before this item closed.
