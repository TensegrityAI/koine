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

## Evidence (pre-review, 2026-07-21)

- Operator contract: `.env.example` names `KOINE_WORKER_TOKEN`,
  `KOINE_GRPC_ADDR`, `KOINE_MAX_LEASE_TTL_MS`, `KOINE_IDLE_POLL_MS`,
  `KOINE_DB_MAX_CONNECTIONS`, and `KOINE_DB_ACQUIRE_TIMEOUT_MS`; the defaults
  are respectively required, `0.0.0.0:7419`, 300000 ms, 1000 ms, 16, and
  5000 ms. The four numeric resource settings reject zero before startup.
- Architecture wiki: `koine-store-postgres`, `koine-server`, and `koine-grpc`
  record an exact `N + 1` per-process Postgres budget (`N` operational-pool
  connections plus one dedicated listener), a single shared listener with
  broadcast fan-out, the 100 ms best-effort presence budget, the idle-poll
  dispatch-recheck fallback, and the phase-3 relay/sink capacity-review
  warning. The Postgres page records that intermediate signal-clone drops keep
  the hub alive and the last drop releases its listener.
- Fresh resource gate: `rg -n "PgSignal::new|PgPool::connect\\(" crates`
  returned no matches (exit 1); `cargo test -p koine-store-postgres` passed
  30 integration tests; `cargo test -p koine-grpc --test grpc_e2e` passed 2;
  `cargo test -p koine-server` passed 10; `make ci` passed; and
  `git diff --check` passed.

## Pending independent review and closure

Step 3 must still obtain independent spec-compliance and quality verdicts,
including reproduction of the size-one listener-pressure and saturated-presence
tests and inspection of design §5 and legacy AC4. Step 4 must then record the
reviewed closure evidence, spec-fidelity statement, and lifecycle moves. No
acceptance criterion is checked and this record remains `ongoing` until then.
