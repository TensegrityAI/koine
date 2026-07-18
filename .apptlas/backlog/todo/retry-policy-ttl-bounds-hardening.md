# Validate RetryPolicy/TTL bounds at the application boundary

- **State:** todo
- **Origin:** phase 1A final review (2026-07-18) — findings b'/d/g share one root
  cause: unvalidated client-supplied bounds
- **Epic:** ../epics/phase-1-event-sourced-core.md (1B hardening)

## Traceability

- **Implements:** hardening of spec §3 delivery semantics; ADR 0011 contracts

## Acceptance criteria

- [ ] AC1: `EnqueueCommand` rejects pathological `RetryPolicy` values (delays
  beyond a sane ceiling, zero/absurd max_attempts) with a typed error —
  *verify:* ring-2 test
- [ ] AC2: the sweep skips only `IllegalTransition` (state moved); other
  domain errors (e.g. `InvalidTtl`) surface instead of silently stranding a
  lease — *verify:* ring-2 test with a poisoned policy
- [ ] AC3: `extend_lease` TTL-overflow philosophy matches `lease` (error, not
  saturate) or the asymmetry is documented in ADR 0011 — *verify:* unit test
- [ ] AC4: fixed-seed cross-attempt jitter variance test added to ring 1;
  seed-entropy expectation documented on `IdGenerator::jitter_seed`
- [ ] AC5: store contract test covers fold-rejected append against an
  EXISTING stream (prior events survive, batch discarded) — inherited by the
  1B Postgres adapter suite

## Dependencies

- Plan 1B (the Postgres adapter inherits AC5's contract test)

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
