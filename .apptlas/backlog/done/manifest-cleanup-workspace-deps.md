# Centralize internal deps and clean manifest descriptions

- **State:** done
- **Origin:** phase 0 final review (2026-07-17), findings a+b
- **Epic:** [Phase 2 — Data plane](../../epics/phase-2-data-plane.md)

## Traceability

- **Implements:** [operational-closure plan Tasks 4 and 5](../../../docs/superpowers/plans/2026-07-21-koine-operational-closure.md) and the phase-0 manifest findings; package boundaries also follow [ADR 0017](../../../docs/adr/0017-hermetic-protobuf-ci-artifact-pinning.md).

## Acceptance criteria

- [x] AC1: every internal crate identity is declared once in root
  `[workspace.dependencies]` with path and version, and consumers use
  `workspace = true` without changing dependency semantics — *verify:*
  normalized pre/post `cargo metadata` diff.
- [x] AC2: every crate description is free of literal Markdown backticks —
  *verify:* manifest scan.
- [x] AC3: every crate is explicitly non-publishable until a phase-2B
  publication decision — *verify:* manifest scan.
- [x] AC4: the seven implemented package file lists contain their required
  sources, migrations/proto assets, and regular `LICENSE`/`NOTICE` copies —
  *verify:* `cargo package --allow-dirty --list -p <crate>` inspection.
- [x] AC5: the centralized graph builds and passes workspace test, dependency,
  and unused-dependency gates — *verify:* `cargo build --workspace`,
  `cargo test --workspace`, `cargo deny check`, and `cargo machete`.

## Dependencies

- [Close phase-2A operational and supply-chain debt](../ongoing/phase-2a-operational-closure.md)
  remains ongoing; this legacy manifest item closes on its own evidence while
  publication and phase 2B stay blocked.

## Historical opening record

This item opened in commit `a3fb420` as a seven-line phase-0 finding. Its
original AC required centralized internal path+version declarations,
`workspace = true` consumers, and descriptions without literal backticks. Its
verification named `cargo build`, `cargo test`, `cargo deny check`, and an
identical pre/post `cargo metadata` dependency graph; its timing was “before
first crates.io publish (phase 2).” AC1, AC2, and AC5 preserve those exact
requirements. AC3 and AC4 are the approved Operational Task 4 publication and
package-boundary gates, added before that task executed.

## Evidence

- `git diff 42a7f3a..fc4a651 -- Cargo.toml crates/*/Cargo.toml compose.yaml
  crates/koine-store-postgres/tests/support/mod.rs
  crates/koine-grpc/tests/support/mod.rs .github/scripts` is the reviewed Task
  4 range. Commit `9888c7c` centralizes internal edges and publication intent;
  `1d8e1ed`, `6304cbc`, and `fc4a651` close legal-file, crate-directory, and
  executable-consumer review gaps.
- The plan's normalized dependency projection produced identical before/after
  SHA-256 values
  `020ed2c8dd4cf9a41978a6465c042f53a375896932082101d1f09cdcb5c658c5`;
  `diff -u /tmp/koine-deps-before.json /tmp/koine-deps-after.json` exited 0
  with no output. The augmented semantic projection also matched at
  `e42c4ae18dc934ef3d321c3453284668c2b2647f001625aa969def95b1d40cb4`.
- The exact manifest scans below produced no output. Root
  `[workspace.dependencies]` contains all eleven internal path+version
  identities, and all consumers inherit them with `workspace = true`.

  ```bash
  rg --files-without-match '^publish = false$' crates/*/Cargo.toml
  rg -n '^description = ".*`' crates/*/Cargo.toml
  ```

- `cargo package --allow-dirty --list -p <crate>` inspected the seven
  implemented crates without claiming that publication is enabled: domain 14
  files, application 16, proto 9 (including `build.rs` and `worker.proto`),
  memory 12, Postgres 23 (including both migrations), gRPC 13, and server 11.
  Every list includes `LICENSE` and `NOTICE`; the semantic gate rejects absent,
  drifted, or symlinked copies.
- `cargo build --workspace`, `cargo test --workspace`, `cargo deny check`, and
  `cargo machete` passed after centralization. Operational Task 4's full
  `make ci` also passed. Exact package lists, diff hashes, commands, and
  caveats are recorded in the
  [Operational Task 4 report](../../../.superpowers/sdd/operational-task-4-report.md).

## 2026-07-22 review-evidence amendment

The durable slice-review verdict is recorded in the parent operational item's
[Slice review evidence](../ongoing/phase-2a-operational-closure.md#slice-review-evidence-2026-07-22),
with the updated local Task 4 report retaining the detailed command evidence.
The exact verdict is: **Spec: Faithful. Quality: Approved — 0 findings.** This
slice verdict supports the checked legacy ACs but does not check the parent
operational ACs or satisfy Task 6's integrated review.

## Spec-fidelity statement

Faithful to the original phase-0 findings a+b and the approved Operational
Task 4 amendment. Dependency kind, rename, features, default features, target,
optionality, registry, external edges, and runtime behavior are unchanged.
Publication remains disabled pending a separate phase-2B decision.
