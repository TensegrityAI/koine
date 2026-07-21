# Close phase-2A operational and supply-chain debt

- **State:** ongoing
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../../epics/phase-2-data-plane.md

## Traceability

- **Implements:** [hardening design §§6–10](../../../docs/superpowers/specs/2026-07-21-koine-phase-2a-zero-debt-hardening-design.md); [ADR-0017](../../../docs/adr/0017-hermetic-protobuf-ci-artifact-pinning.md); [operational-closure plan Tasks 1–6](../../../docs/superpowers/plans/2026-07-21-koine-operational-closure.md); closes the CI-pinning and manifest-cleanup legacy items.

## Acceptance criteria

- [ ] AC1: repository-owned actions/downloads/tooling satisfy the accepted immutable-input policy and a fail-closed semantic gate rejects floating, parser, filesystem, and source-form bypasses — *verify:* `make supply-chain` plus its repository-owned mutation suite.
- [ ] AC2: protobuf builds with a deliberately invalid `PROTOC`, proving vendored compiler selection — *verify:* isolated-target `cargo build -p koine-proto`.
- [ ] AC3: internal dependency edges are identical before/after centralization; descriptions contain no backticks and every crate is non-publishable — *verify:* normalized metadata diff and manifest scan.
- [ ] AC4: every implemented crate's package file list contains required sources/assets/licenses — *verify:* `cargo package --allow-dirty --list -p ...`.
- [ ] AC5: README, roadmap, living context, epic, env reference, architecture wiki, and backlog agree about present/future behavior — *verify:* docs/spec review.
- [ ] AC6: formal, full CI, real Postgres dev-loop, real TCP/Postgres gRPC, server startup/shutdown, and zero-debt audit are fresh green — *verify:* final gate commands.

## Dependencies

- [Make lease retirement atomic with heartbeat renewal](../done/phase-2a-atomic-lease-retirement.md) — **State:** done.
- [Bound Postgres resources on the phase-2A server](../done/phase-2a-postgres-resource-safety.md) — **State:** done.

## Temporary operational exception

- `compose.yaml` retains exactly `postgres:17` without a digest. **Owner:**
  Operational Task 4. **Deadline:** before this item can close. The
  supply-chain gate permits only that exact temporary value; Task 4 replaces
  it with the approved digest and removes the exception branch.

## Evidence (filled at close)

- Operational Task 2 current evidence is recorded in
  [the supply-chain report](../../../.superpowers/sdd/operational-task-2-report.md).
  AC1 is implemented and independently reviewable, but remains unchecked until
  the parent operational item completes all acceptance criteria and DoD gates.
- Operational Task 3 current evidence is recorded in
  [the hermetic protobuf report](../../../.superpowers/sdd/operational-task-3-report.md).
  A fresh isolated target fails with the pre-change build and an invalid
  `PROTOC`; after selecting exact `protoc-bin-vendored` 3.2.0 directly, a
  second fresh target builds with the same poisoned environment. AC2 remains
  unchecked until independent review and parent-item closure.

## Spec-fidelity statement (filled at close)
