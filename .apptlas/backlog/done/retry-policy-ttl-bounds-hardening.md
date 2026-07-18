# Validate RetryPolicy/TTL bounds at the application boundary

- **State:** done
- **Origin:** phase 1A final review (2026-07-18) — findings b'/d/g share one root
  cause: unvalidated client-supplied bounds
- **Epic:** ../epics/phase-1-event-sourced-core.md (1B hardening)

## Traceability

- **Implements:** hardening of spec §3 delivery semantics; ADR 0011 contracts

## Acceptance criteria

- [x] AC1: `EnqueueCommand` rejects pathological `RetryPolicy` values (delays
  beyond a sane ceiling, zero/absurd max_attempts) with a typed error —
  *verify:* `cargo test -p koine-store-memory --test lifecycle
  enqueue_rejects_pathological_retry_policies` (ring 2; covers
  `max_attempts == 0`, `max_attempts` above the 10,000 ceiling, `base_delay >
  max_delay`, and `max_delay` above the 30-day ceiling — all four rejected as
  `EnqueueError::InvalidPolicy`)
- [x] AC2: the sweep skips only `IllegalTransition` (state moved); other
  domain errors (e.g. `InvalidTtl`) surface instead of silently stranding a
  lease — *verify:* `sweep_surfaces_non_transition_domain_errors`, present
  test-for-test in BOTH `cargo test -p koine-store-memory --test lifecycle`
  (ring 2) and `cargo test -p koine-store-postgres --test lifecycle` (ring 3
  mirror, real Postgres) — a poisoned policy (`base_delay`/`max_delay` =
  `Duration::MAX`) that folds fine but overflows chrono at decision time
  surfaces as `SweepError::Domain`, not a silent `continue`
- [x] AC3: `extend_lease` TTL-overflow philosophy matches `lease` (error, not
  saturate) — *verify:* `cargo test -p koine-store-memory --lib
  extend_lease_rejects_unrepresentable_ttl` (`Duration::MAX` rejected as
  `DispatchError::Backend`, ring 2); Postgres parity verified by code
  inspection — `PostgresDispatcher::extend_lease` (`src/dispatcher.rs`) runs
  the identical `chrono::TimeDelta::from_std(ttl)` guard and returns the same
  `DispatchError::Backend("ttl out of range")` on failure. No dedicated
  ring-3 test drives a `Duration::MAX` extension (recorded gap, see
  Evidence) — the parity claim rests on the shared guard, not an executed
  ring-3 assertion.
- [x] AC4: fixed-seed cross-attempt jitter variance test added to ring 1;
  seed-entropy expectation documented on `IdGenerator::jitter_seed` —
  *verify:* `cargo test -p koine-domain --lib
  different_attempts_can_differ_for_fixed_seed` (`src/retry.rs`; asserts a
  fixed seed still produces >4 distinct delays across 15 attempts); doc:
  `koine-application/src/ports.rs`'s `jitter_seed` doc comment ("Implementations
  MUST return high-entropy values (e.g. from the id source); small
  sequential counters would correlate delays across jobs (`seed ^ attempt`
  collisions)"), realized by `koine-server`'s `UuidV7Ids::jitter_seed`
  (folds both 64-bit halves of a fresh UUIDv7) and `koine-store-memory`'s
  `SeededIds` test double.
- [x] AC5: store contract test covers fold-rejected append against an
  EXISTING stream (prior events survive, batch discarded) — inherited by the
  1B Postgres adapter suite — *verify:* BOTH stores carry the contract:
  `koine-store-memory/src/store.rs`'s
  `fold_rejected_append_leaves_no_trace` (fresh stream) and
  `fold_rejected_append_on_existing_stream_keeps_prior_events` (existing
  stream) as two separate unit tests; `koine-store-postgres/tests/store.rs`'s
  `failed_append_leaves_no_trace_fresh_or_existing` as one ring-3 test
  covering both cases against real Postgres.

## Dependencies

- Plan 1B (the Postgres adapter inherits AC5's contract test) — satisfied;
  Postgres adapter delivered in the same phase this item closes alongside.

## Evidence (filled at close)

All five ACs implemented in phase-1B Tasks 2–3
(`docs/superpowers/plans/2026-07-18-koine-phase-1b-postgres-store.md`) and
verified again at this closeout via `cargo test --workspace` (75 tests, 0
failed) plus the targeted runs named per AC above. Commits: `ef7b75b` (test:
harden jitter, ttl symmetry, and fold-reject contracts — AC3/AC4/AC5) and
`d63ad73` (feat(application): validate retry-policy bounds and surface sweep
faults — AC1/AC2, plus the `checked_add_signed` domain fix this hardening
work surfaced).

**Recorded gap (not blocking closure):** AC3's Postgres side has no
dedicated ring-3 test exercising `extend_lease` with an unrepresentable TTL
— the claim is code-parity (same guard, same error path), not an executed
assertion against real Postgres. A candidate follow-up (not filed as its own
item, since it is a coverage nice-to-have rather than a known defect) would
add a `Duration::MAX` case to `koine-store-postgres/tests/dispatcher.rs`.

## Spec-fidelity statement (filled at close)

Faithful — this item hardens ADR 0011's contracts (enqueue-time validation,
sweep error discipline, lease TTL symmetry, jitter entropy, fold-reject
side-effect-freedom) without changing any of them. One correctness bug was
found and fixed while implementing AC1/AC2, recorded in
`phase-1b-postgres-store.md`'s fidelity section rather than duplicated here:
`Job`'s retry-decision path used a panicking `now + delay` instead of
`now.checked_add_signed(delay)`, which this item's own poisoned-policy test
(`sweep_surfaces_non_transition_domain_errors`) exercises directly.
