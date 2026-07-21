# Bound Postgres resources on the phase-2A server

- **State:** done
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§3, 5, 9–10; resource-hardening plan Tasks 1–5; closes phase-2-carryover-hardening AC4.

## Acceptance criteria

- [x] AC1: `connect_pool` honors explicit non-zero max connections and acquisition timeout — *evidence:* ring-3 `pool_options_are_honored` proves `PoolConfig(2, 750 ms)` reaches `PgPoolOptions`; `koine-server` parse tests cover default and override values.
- [x] AC2: zero TTL, idle poll, pool size, and acquisition timeout fail before startup — *evidence:* `koine-server` unit test `zero_resource_values_are_rejected` covers all four variables; malformed pool-size and acquisition-timeout inputs are covered by `invalid_pool_size_is_rejected` and `invalid_acquire_timeout_is_rejected`.
- [x] AC3: any number of idle waits share one dedicated listener and leave a size-one operational pool available — *evidence:* ring-3 `thirty_two_waiters_share_one_listener_without_starving_append` coordinates 32 waits, observes one `LISTEN` backend, and appends through a size-one operational pool.
- [x] AC4: listener reconnect preserves prompt wakeup; loss still falls back to `idle_poll` — *evidence:* ring-3 `listener_reconnects_after_backend_termination` observes a replacement listener and prompt wakeup; `koine-grpc` wire test `fetch_wakes_on_late_enqueue` exercises the signal path while `idle_poll` remains the documented dispatch-recheck fallback.
- [x] AC5: saturated-pool presence returns promptly and never fails the worker request — *evidence:* ring-3 `presence_skips_when_pool_is_saturated` holds the sole operational connection and proves the best-effort presence call returns within its external 250 ms detector; implementation uses immediate `try_acquire` and a 100 ms acquired-write budget.
- [x] AC6: env/reference/wiki docs state defaults, constraints, total connection budget, and phase-3 relay/sink risk — *evidence:* `.env.example` and all three architecture pages state the non-zero defaults, exact `N + 1` budget, listener fan-out/lifecycle, 100 ms presence contract, idle-poll fallback, and phase-3 capacity review; independently reviewed and covered by final `make ci`.

## Dependencies

- [Atomic lease retirement](../done/phase-2a-atomic-lease-retirement.md) — done.

## Evidence (closed, 2026-07-21)

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
  `git diff --check` passed. Final closure re-ran `make ci` and
  `git diff --check`; the current workspace inventory is 127 tests (not the
  historical 126-test base-slice total).

## Independent review verdict (2026-07-21)

- Spec compliance: ✅ Faithful to hardening design §5 and legacy carryover AC4.
- Quality: Approved — no Critical, Important, or unrecorded Minor findings.

The independent reviewer reproduced the size-one pressure, saturated-presence,
listener-reconnect, and final-clone-drop tests, inspected design §5 and legacy
AC4, and re-reviewed the M-001/M-002 documentation corrections.

## Spec-fidelity statement

Faithful to hardening design §§3, 5, and 9–10 and legacy carryover AC4. No
specification, ADR, event, wire, schema, or HA-scope divergence was absorbed.
Phase 2B remains blocked by its remaining cross-cutting hardening slices; this
resource closure does not open or implement phase 2B.
