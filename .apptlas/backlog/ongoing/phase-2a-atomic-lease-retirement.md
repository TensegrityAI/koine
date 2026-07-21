# Make lease retirement atomic with heartbeat renewal

- **State:** todo
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§3–4, 9–10; ADR-0016; atomic-lease plan Tasks 1–5.

## Acceptance criteria

- [ ] AC1: the TLA+ model includes time, deadline, lease identity, bounded heartbeats, and `HeartbeatExpiryFence`; TLC passes and the documented stale-expiry mutation fails that invariant — *verify:* `make tla` plus mutation evidence in `docs/formal/README.md`.
- [ ] AC2: `Dispatcher` exposes atomic `retire_next_expired_lease`; no `expired` list contract remains — *verify:* `rg -n "fn expired|\.expired\(" crates` returns no matches and ring 2 is green.
- [ ] AC3: heartbeat-first preserves the renewal, retirement-first rejects heartbeat, and two sweepers retire one grant once in memory and Postgres — *verify:* named ring-2/ring-3 regressions.
- [ ] AC4: expiry/retry events still come from `Job::expire_lease`, retain lineage, and late ACK remains a conflict — *verify:* mirrored lifecycle suites and gRPC e2e.
- [ ] AC5: architecture docs and every sweep call site describe/use the atomic contract — *verify:* docs review, `make ci`, `make tla`.

## Dependencies

- Approved ADR-0016 and hardening design; both accepted 2026-07-21.

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
