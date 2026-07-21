# Bound Postgres resources on the phase-2A server

- **State:** ongoing
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§3, 5, 9–10; resource-hardening plan Tasks 1–5; closes phase-2-carryover-hardening AC4.

## Acceptance criteria

- [ ] AC1: `connect_pool` honors explicit non-zero max connections and acquisition timeout — *verify:* ring-3 `pool_options_are_honored` and server parse tests.
- [ ] AC2: zero TTL, idle poll, pool size, and acquisition timeout fail before startup — *verify:* `koine-server` unit tests.
- [ ] AC3: any number of idle waits share one dedicated listener and leave a size-one operational pool available — *verify:* ring-3 fan-out pressure test.
- [ ] AC4: listener reconnect preserves prompt wakeup; loss still falls back to `idle_poll` — *verify:* ring-3 reconnect and existing gRPC wakeup tests.
- [ ] AC5: saturated-pool presence returns promptly and never fails the worker request — *verify:* ring-3 `presence_skips_when_pool_is_saturated`.
- [ ] AC6: env/reference/wiki docs state defaults, constraints, total connection budget, and phase-3 relay/sink risk — *verify:* docs review and `make ci`.

## Dependencies

- [Atomic lease retirement](../done/phase-2a-atomic-lease-retirement.md) — done.

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
