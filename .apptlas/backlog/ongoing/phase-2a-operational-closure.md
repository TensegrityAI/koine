# Close phase-2A operational and supply-chain debt

- **State:** ongoing
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../../epics/phase-2-data-plane.md

## Traceability

- **Implements:** [hardening design §§6–10](../../../docs/superpowers/specs/2026-07-21-koine-phase-2a-zero-debt-hardening-design.md); [ADR-0017](../../../docs/adr/0017-hermetic-protobuf-ci-artifact-pinning.md); [operational-closure plan Tasks 1–6](../../../docs/superpowers/plans/2026-07-21-koine-operational-closure.md); closes the CI-pinning and manifest-cleanup legacy items.

## Acceptance criteria

- [ ] AC1: repository-owned actions/downloads/tooling satisfy the accepted immutable-input policy and an automated gate rejects floating regressions — *verify:* `make supply-chain` plus deliberate mutation probe.
- [ ] AC2: protobuf builds with a deliberately invalid `PROTOC`, proving vendored compiler selection — *verify:* isolated-target `cargo build -p koine-proto`.
- [ ] AC3: internal dependency edges are identical before/after centralization; descriptions contain no backticks and every crate is non-publishable — *verify:* normalized metadata diff and manifest scan.
- [ ] AC4: every implemented crate's package file list contains required sources/assets/licenses — *verify:* `cargo package --allow-dirty --list -p ...`.
- [ ] AC5: README, roadmap, living context, epic, env reference, architecture wiki, and backlog agree about present/future behavior — *verify:* docs/spec review.
- [ ] AC6: formal, full CI, real Postgres dev-loop, real TCP/Postgres gRPC, server startup/shutdown, and zero-debt audit are fresh green — *verify:* final gate commands.

## Dependencies

- [Make lease retirement atomic with heartbeat renewal](../done/phase-2a-atomic-lease-retirement.md) — **State:** done.
- [Bound Postgres resources on the phase-2A server](../done/phase-2a-postgres-resource-safety.md) — **State:** done.

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
