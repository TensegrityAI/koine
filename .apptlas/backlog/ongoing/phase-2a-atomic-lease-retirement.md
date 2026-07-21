# Make lease retirement atomic with heartbeat renewal

- **State:** ongoing
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§3–4, 9–10; ADR-0016; atomic-lease plan Tasks 1–5.

## Acceptance criteria

- [x] AC1: the TLA+ model includes time, deadline, lease identity, bounded heartbeats, and `HeartbeatExpiryFence`; TLC passes and the documented stale-expiry mutation fails that invariant — *verify:* `make tla` plus mutation evidence in `docs/formal/README.md`.
- [ ] AC2: `Dispatcher` exposes atomic `retire_next_expired_lease`; no `expired` list contract remains — *verify:* `rg -n "fn expired|\.expired\(" crates` returns no matches and ring 2 is green.
- [ ] AC3: heartbeat-first preserves the renewal, retirement-first rejects heartbeat, and two sweepers retire one grant once in memory and Postgres — *verify:* named ring-2/ring-3 regressions.
- [ ] AC4: expiry/retry events still come from `Job::expire_lease`, retain lineage, and late ACK remains a conflict — *verify:* mirrored lifecycle suites and gRPC e2e.
- [ ] AC5: architecture docs and every sweep call site describe/use the atomic contract — *verify:* docs review, `make ci`, `make tla`.

## Dependencies

- Approved ADR-0016 and hardening design; both accepted 2026-07-21.

## Evidence (technical gate prepared 2026-07-21; closure still pending independent review)

- Task 2 formal RED (`make tla`): the deliberate stale/early-class expiry
  guard violated `HeartbeatExpiryFence` at graph depth 4 after
  `Init -> Lease -> Heartbeat -> Expire`. This minimal witness is
  early-after-accepted-heartbeat: `Lease` and `Heartbeat` both occur at
  `now = 0`, so the deadline remains `2` rather than moving, but lease `1` is
  retired at `now = 0`. The mutant ignores the current deadline and represents
  the broader stale/early defect class; this trace does not show a displaced
  deadline. TLC generated 19 states, found 16 distinct, and left 10 queued.
- Task 2 formal GREEN (`make tla`): fencing `Expire` with `now >= deadline`
  completed with no error under the same invariants, fairness, and bounds;
  74,079 states generated, 18,598 distinct, zero queued, graph depth 24.

- TDD and mutation trace: commit `6aa29e1` introduced the heartbeat-aware
  formal model and documents the deliberate stale/early expiry mutation above;
  the fixed guard is `now >= deadline`. The memory atomic implementation is
  `a4029bc`; its ordering regressions are recorded in `e9de75d`. The Postgres
  fence and its initial regression set are `d39f022`, with controlled
  lock-overlap regressions in `99bb098`. These history references are evidence
  for review, not a self-certified TDD or review verdict.
- Atomic contract: `Dispatcher::retire_next_expired_lease` is the only sweep
  operation; `SweepExpiredLeases` loops it until `None`. The required source
  scan is recorded below rather than inferred from this prose.
- Ring 2 regressions: `heartbeat_first_fences_retirement` proves an accepted
  renewal leaves the stream at two events and prevents retirement;
  `retirement_first_rejects_heartbeat_and_happens_once` proves a retired grant
  cannot renew and is retired once. Ring 3 regressions:
  `heartbeat_first_fences_retirement`, `retirement_first_rejects_heartbeat`,
  `concurrent_retirement_records_one_expiry`,
  `skip_locked_retires_second_expired_lease`, and
  `locked_expired_row_does_not_beat_earlier_heartbeat` cover both lock orders,
  exactly-once retirement, and unrelated-row `SKIP LOCKED` progress.
- Invariants retained: retirement calls `Job::expire_lease`, so expiry/retry
  events and lineage continue through the aggregate; heartbeats remain
  ephemeral. ADR-0016 leaves the event taxonomy and `koine.v1` wire contract
  unchanged. `EventuallySettled` is conditional on the formal model's finite
  heartbeat allowance (`MaxHeartbeats = 2`), not an unconditional production
  settlement promise.

## Fresh gate results (2026-07-21)

- `rg -n "fn expired|\.expired\(" crates` — exit 1, no output (the expected
  no-match result).
- `make tla` — exit 0: 74,079 generated / 18,598 distinct / 0 queued / depth
  24; TLC completed with no error.
- `make ci` — exit 0: fmt, clippy, the 119 current workspace unit/integration
  tests, rustdoc, `cargo deny`, typos, markdownlint, and cargo-machete passed.
  `cargo deny` emitted its existing duplicate-crate warnings but finished
  `advisories ok, bans ok, licenses ok, sources ok`.
- `git diff --check` — exit 0, no output.

## Deliberately open before closure

- Independent review verdicts required by DoD item 8: a reviewer must read
  ADR-0016 and hardening design §§3–4, reproduce TLC and the controlled race,
  then record both spec-compliance and quality verdicts. No implementer
  self-certification is recorded here.
- Final spec-fidelity statement is reserved for that review outcome.
- The item remains in `ongoing/`; do not move it to `done/` until the two
  verdicts and final spec-fidelity statement are present.

## Spec-fidelity statement (filled at close)
