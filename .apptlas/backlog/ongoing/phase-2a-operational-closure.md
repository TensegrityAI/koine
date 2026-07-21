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
- Operational Task 4 current evidence is recorded in
  [the image, manifest, and package report](../../../.superpowers/sdd/operational-task-4-report.md).
  The approved Postgres digest now covers Compose and both real test harnesses;
  the semantic gate independently enforces exactly one approved consumer in
  each versioned Rust helper; and its crate scan requires the exact eleven real
  directories without following symlinks or accepting other entry types;
  normalized internal dependency edges are byte-identical; all crate manifests
  are non-publishable; and the seven implemented package lists contain their
  required sources, assets, `LICENSE`, and `NOTICE`. AC3 and AC4 remain
  unchecked until independent review and parent-item closure.

## Spec-fidelity statement (filled at close)

## 2026-07-22 Task 5 truth-reconciliation amendment

The earlier evidence paragraphs are preparatory snapshots from their
individual cuts. Their acceptance checkboxes correctly remain open at parent
scope, but their references to pending independent review are now historical:
Operational Tasks 2, 3, and 4 each passed their recorded independent reviews.

- Public, living, epic, environment, architecture, and formal documentation
  now use the live state: phase 2A implementation complete; zero-debt hardening active; phase 2B blocked. Phase-2B scope remains explicit and
  unimplemented.
- The two phase-0 legacy findings are closed at
  [ci-supply-chain-pinning.md](../done/ci-supply-chain-pinning.md) and
  [manifest-cleanup-workspace-deps.md](../done/manifest-cleanup-workspace-deps.md).
  Each retains its historical opening requirement, checks template-complete
  current ACs, records exact implementation/package evidence, and states
  `Faithful`.
- The current semantic supply-chain suite has 73 probes. Older counts and
  hashes remain only where explicitly labeled historical/superseded; current
  identities remain canonical in the Operational Task 2 and Task 4 reports.
- Task 5 evidence is recorded in
  [the truth-reconciliation report](../../../.superpowers/sdd/operational-task-5-report.md).

AC1–AC6 above remain unchecked and this item remains `ongoing`. Task 5 does
not run or claim Task 6's fresh product exit gate, independent parent review,
or phase-unblock decision.
