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
  unchecked until independent review and parent-item closure. **Historical
  preparatory wording: slice review is complete; the parent checkbox remains
  open until Task 6.**
- Operational Task 4 current evidence is recorded in
  [the image, manifest, and package report](../../../.superpowers/sdd/operational-task-4-report.md).
  The approved Postgres digest now covers Compose and both real test harnesses;
  the semantic gate independently enforces exactly one approved consumer in
  each versioned Rust helper; and its crate scan requires the exact eleven real
  directories without following symlinks or accepting other entry types;
  normalized internal dependency edges are byte-identical; all crate manifests
  are non-publishable; and the seven implemented package lists contain their
  required sources, assets, `LICENSE`, and `NOTICE`. AC3 and AC4 remain
  unchecked until independent review and parent-item closure. **Historical
  preparatory wording: slice review is complete; the parent checkboxes remain
  open until Task 6.**

### Operational Task 6 first-wave evidence (2026-07-22)

This evidence was collected on `feat/phase-2a-hardening` at `d207b9e`, before
the Task 6 evidence commit. Raw command output and per-command metadata are in
`/tmp/koine-operational-task-6.6Ciq9i`; the ignored
`.superpowers/sdd/operational-task-6-report.md` records the exhaustive ledger.

The exact automated gate sequence produced:

| Command | Exit | Elapsed | Fresh result |
| --- | ---: | ---: | --- |
| `make supply-chain` | 0 | 3.447 s | 73/73 repository-owned probes passed; npm audited 88 packages with 0 vulnerabilities |
| `make tla` | 0 | 1.048 s | 74,079 states generated; 18,598 distinct; 0 queued; depth 24; no error |
| `make ci` | 0 | 43.238 s | 127 tests passed and 0 failed; rustdoc, deny, typos, 80-file Markdownlint, machete, and 73/73 supply-chain probes passed |
| `cargo test -p koine-store-postgres` | 0 | 36.117 s | 30 tests passed and 0 failed against real testcontainers Postgres |
| `cargo test -p koine-grpc --test grpc_e2e` | 0 | 8.242 s | 2 tests passed and 0 failed over real TCP and real Postgres: presence plus crash recovery |
| `git diff --check` | 0 | 0.003 s | no output |

The product exercise preserved the prior running-state boundary. Before the
exercise, `docker compose ps --all` had no service and Postgres was not
running. `docker compose config --images` resolved exactly
`postgres:17@sha256:a426e44bac0b759c95894d68e1a0ac03ecc20b619f498a91aae373bf06d8508d`.
`docker compose up -d postgres` exited 0 in 0.420 s, and `pg_isready` reported
ready on its first bounded probe in 0.198 s. The following product commands
then produced:

| Command or check | Exit | Elapsed | Observed result |
| --- | ---: | ---: | --- |
| `cargo run -p koine-server -- dev-loop` | 0 | 3.162 s | plain `enqueued,leased,started,succeeded`; crash `enqueued,leased,lease_expired,retry_scheduled,leased,started,succeeded`; retry `enqueued,leased,started,failed,retry_scheduled,leased,started,succeeded`; all terminal |
| `cargo build -p koine-server` | 0 | 0.108 s | development binary built |
| `KOINE_WORKER_TOKEN=phase2a-smoke target/debug/koine-server serve` | 0 after `SIGINT` and `wait` | 0.145 s | PID 1849185 remained alive; its socket listened on `0.0.0.0:7419`; the authenticated-data-plane phrase appeared; readiness took 2 of 20 bounded 100 ms probes |
| `rg -n 'authenticated grpc data plane' /tmp/koine-serve-smoke.log` | 0 | included above | matched line 1 |
| conditional `docker compose stop postgres` | 0 | 0.396 s | executed because Postgres was not running before the exercise |

The server cleanup trap had no remaining work: PID 1849185 was absent and
port 7419 was no longer listening. Final Compose inspection found no running
Postgres service and the exercise-created container was `Exited (0)`. In
accordance with the non-destructive restoration constraint, no `down`, volume
removal, or network removal ran; the stopped container and its data volume
remain. The final configured image retained the exact reviewed digest.

The exact closed-phase residue commands produced:

| Command | Exit | Output and interpretation |
| --- | ---: | --- |
| `find .apptlas/backlog/todo -maxdepth 1 -type f ! -name .gitkeep -print` | 0 | empty; no todo item |
| `rg -n 'todo!\(\|unimplemented!\(' crates` | 1 | empty; ripgrep found no fake-completeness macro |
| stale/floating-phrase `rg` from Task 6 Step 3 | 1 | empty; no prohibited match |
| `rg -L '^publish = false$' crates/*/Cargo.toml` | 0 | 11 matching lines; in ripgrep `-L` follows symlinks rather than selecting non-matches |
| `git status --short` | 0 | empty before this evidence edit |

The ambiguous short-option command was supplemented without replacing its
exact result: `rg --files-without-match '^publish = false$'
crates/*/Cargo.toml` exited 1 with no output, while `rg -l` found all 11 of 11
workspace manifests. The ongoing inventory contains exactly this operational
item; the todo inventory is empty.

**Independent parent review is pending.** These first-wave observations do not
self-certify Task 6 Step 4, the parent Definition of Done, or any acceptance
criterion. AC1–AC6 remain unchecked, this item remains `ongoing`, and the live
state remains: phase 2A implementation complete; zero-debt hardening active;
phase 2B blocked.

## Spec-fidelity statement (filled at close)

Pending independent Task 6 dual-verdict review; no `Faithful` verdict is
recorded by the implementer.

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

### Slice review evidence (2026-07-22)

- Operational Task 3 (`42a7f3a`), after review of the poisoned-`PROTOC`
  RED/GREEN proof, dependency graph, workspace gates, and architecture update:
  **Spec: Faithful. Quality: Approved — 0 findings.** Its earlier “pending
  independent review” text is historical and superseded by this verdict.
- Operational Task 4 (implementation and corrections through `36b7646`), after
  review of the immutable Postgres consumers, byte-identical dependency graph,
  package boundaries, legal-file integrity, and 73-probe lexer gate:
  **Spec: Faithful. Quality: Approved — 0 findings.** Its earlier “pending
  independent review” text is historical and superseded by this verdict.

These are slice verdicts, not the parent DoD review. AC1–AC6 remain unchecked
until Task 6 supplies the fresh integrated evidence and independent parent
review.

AC1–AC6 above remain unchecked and this item remains `ongoing`. Task 5 does
not run or claim Task 6's fresh product exit gate, independent parent review,
or phase-unblock decision.
