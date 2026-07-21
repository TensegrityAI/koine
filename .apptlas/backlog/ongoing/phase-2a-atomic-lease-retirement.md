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

## Evidence (filled at close)

- Task 2 formal RED (`make tla`): the deliberate stale/early expiry guard
  violated `HeartbeatExpiryFence` at graph depth 4 after
  `Init -> Lease -> Heartbeat -> Expire`. Lease `1` was renewed to deadline
  `2`, then incorrectly retired at `now = 0`; 19 states were generated, 16
  were distinct, and 10 remained queued when TLC found the counterexample.
- Task 2 formal GREEN (`make tla`): fencing `Expire` with `now >= deadline`
  completed with no error under the same invariants, fairness, and bounds;
  74,079 states generated, 18,598 distinct, zero queued, graph depth 24.

## Spec-fidelity statement (filled at close)
