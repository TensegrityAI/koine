# Close phase-2A operational and supply-chain debt

- **State:** done
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../../epics/phase-2-data-plane.md

## Traceability

- **Implements:** [hardening design §§6–10](../../../docs/superpowers/specs/2026-07-21-koine-phase-2a-zero-debt-hardening-design.md); [ADR-0017](../../../docs/adr/0017-hermetic-protobuf-ci-artifact-pinning.md); [operational-closure plan Tasks 1–6](../../../docs/superpowers/plans/2026-07-21-koine-operational-closure.md); closes the CI-pinning and manifest-cleanup legacy items.

## Acceptance criteria

- [x] AC1: repository-owned actions/downloads/tooling satisfy the accepted immutable-input policy and a fail-closed semantic gate rejects floating, parser, filesystem, and source-form bypasses — *verify:* `make supply-chain` plus its repository-owned mutation suite.
- [x] AC2: protobuf builds with a deliberately invalid `PROTOC`, proving vendored compiler selection — *verify:* isolated-target `cargo build -p koine-proto`.
- [x] AC3: internal dependency edges are identical before/after centralization; descriptions contain no backticks and every crate is non-publishable — *verify:* normalized metadata diff and manifest scan.
- [x] AC4: every implemented crate's package file list contains required sources/assets/licenses — *verify:* `cargo package --allow-dirty --list -p ...`.
- [x] AC5: README, roadmap, living context, epic, env reference, architecture wiki, and backlog agree about present/future behavior — *verify:* docs/spec review.
- [x] AC6: formal, full CI, real Postgres dev-loop, real TCP/Postgres gRPC, server startup/shutdown, and zero-debt audit are fresh green — *verify:* final gate commands.

## Dependencies

- [Make lease retirement atomic with heartbeat renewal](../done/phase-2a-atomic-lease-retirement.md) — **State:** done.
- [Bound Postgres resources on the phase-2A server](../done/phase-2a-postgres-resource-safety.md) — **State:** done.

## Evidence (filled at close)

> Evidence-durability note: the per-task working reports referenced below lived
> in the session-local, git-ignored `.superpowers/sdd/` scratch area and were
> not committed, so they are not durable artifacts a later auditor can open.
> The substantive evidence each carried is transcribed inline in this record
> (command tables, verdicts) and is reproducible via the listed `make` targets
> and tests. References below are named for provenance, not as live links.

- Operational Task 2 evidence (session-local supply-chain report, uncommitted;
  transcribed inline below and reproducible via `make supply-chain`).
  AC1 is implemented and independently reviewable, but remains unchecked until
  the parent operational item completes all acceptance criteria and DoD gates.
- Operational Task 3 evidence (session-local hermetic-protobuf report,
  uncommitted; transcribed inline and reproducible via `cargo build -p
  koine-proto` with a poisoned `PROTOC`).
  A fresh isolated target fails with the pre-change build and an invalid
  `PROTOC`; after selecting exact `protoc-bin-vendored` 3.2.0 directly, a
  second fresh target builds with the same poisoned environment. AC2 remains
  unchecked until independent review and parent-item closure. **Historical
  preparatory wording: slice review is complete; the parent checkbox remains
  open until Task 6.**
- Operational Task 4 evidence (session-local image/manifest/package report,
  uncommitted; transcribed inline below).
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

### Operational Task 6 first-wave evidence (historical pre-review snapshot)

This evidence was collected on `feat/phase-2a-hardening` at `d207b9e`, before
the Task 6 evidence commit. Raw command output and per-command metadata lived
in a session-local temp dir and the git-ignored
`.superpowers/sdd/operational-task-6-report.md`; neither was committed. The
durable ledger is the transcribed command table that follows, reproducible via
the same commands.

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

At this first-wave checkpoint, independent parent review was pending. The
observations did not self-certify Task 6 Step 4, the parent Definition of Done,
or any acceptance criterion; AC1–AC6 remained unchecked and the item remained
`ongoing`. The independent review and closure evidence below supersede only
that pending status, not the recorded command results.

### Independent parent review and closure evidence (2026-07-22)

The independent Step 4 reviewer read the approved designs, ADRs, policies, all
three hardening plans, cumulative 45-commit diff, task evidence, and the
high-risk implementation surfaces. Its working notes were session-local
scratch (git-ignored `.superpowers/sdd/`, not committed); the reviewer's dual
verdict against reviewed HEAD `3b0152e` is transcribed immediately below, which
is the durable record of it.

Spec compliance: ✅ Faithful to the approved Phase 2A zero-debt hardening design and ADR-0016/ADR-0017 as amended.

Quality: Approved — no Critical, Important, or Minor findings attributable to closed phases.

Exit decision: Approved to close Phase 2A and begin Phase 2B planning; Phase 2B implementation is not authorized.

The reviewer independently reproduced:

| Exact command | Exit | Elapsed | Result |
| --- | ---: | ---: | --- |
| `make supply-chain` | 0 | 3.938 s | 73/73 probes; 88 npm packages audited; 0 vulnerabilities |
| `make tla` | 0 | 1.037 s | 74,079 generated; 18,598 distinct; 0 queued; depth 24; no error |
| `cargo test -p koine-store-postgres --test dispatcher --test signal` (first run) | 101 | 20.702 s | dispatcher 9/9; signal 6/7; environmental `PortNotExposed` before the affected test body |
| same exact Postgres command (confirmation) | 0 | 15.337 s | dispatcher 9/9 and signal 7/7; 16/16 total |
| `cargo test -p koine-grpc --test grpc_e2e` | 0 | 6.115 s | 2/2 over real TCP and Postgres |
| `PROTOC=/definitely/missing/protoc CARGO_TARGET_DIR=<fresh> cargo build -p koine-proto` | 0 | 6.942 s | poisoned-host fresh build succeeded; validated temporary target removed |
| `cargo metadata --format-version 1 --no-deps \| jq <extended-projection>` | 0 | 0.023 s | 11 packages; exact expected SHA-256 `e42c4ae18dc934ef3d321c3453284668c2b2647f001625aa969def95b1d40cb4` |
| `git diff --check b0752cb..HEAD` | 0 | <0.1 s | no output |

The first targeted Postgres run's only error was testcontainers
`PortNotExposed` while resolving TCP 5432 from a just-started container.
Normal harness cleanup had already removed that container, and no Koiné
assertion or product path ran. The exact isolated test then passed 1/1, the
full signal binary passed 7/7, three further signal repetitions passed 7/7,
and the exact combined confirmation passed 16/16. This is recorded as a
non-reproducing Docker/testcontainers environment event, not a closed-phase
finding; it is not a current failing gate.

The checked ACs map to exact evidence:

- **AC1:** Task 6 `make supply-chain` passed 73/73 in 3.447 s; independent
  reproduction passed 73/73 in 3.938 s. The reviewed semantic checker,
  identities, and mutation coverage satisfy ADR-0017 as amended.
- **AC2:** Operational Task 3 records the poisoned-`PROTOC` RED/GREEN proof;
  the reviewer independently built `koine-proto` from a fresh target with
  `PROTOC=/definitely/missing/protoc`, exit 0 in 6.942 s.
- **AC3:** the extended current metadata projection is byte-identical to its
  captured expected projection and hashes to
  `e42c4ae18dc934ef3d321c3453284668c2b2647f001625aa969def95b1d40cb4`;
  the manifest scan covers all 11 non-publishable crates and compliant
  descriptions.
- **AC4:** the reviewer reran all seven implemented-crate package lists: 14,
  16, 9, 12, 23, 13, and 11 files respectively. Every list contains its
  source/build inputs plus `LICENSE` and `NOTICE`; proto and Postgres lists
  also contain the required contract and two migrations.
- **AC5:** independent present-versus-future review found README, living
  context, roadmap, epic, environment, architecture, formal documentation,
  and backlog truthful at the review boundary. This closure change advances
  only the approved phase state and retains all phase-2B/later scope as future.
- **AC6:** the first wave records fresh formal, CI, Postgres, gRPC, dev-loop,
  serve/startup/shutdown, and residue evidence. The reviewer additionally
  reproduced supply-chain, TLC, targeted Postgres concurrency/resource tests,
  and real TCP/Postgres gRPC without a reproducible product failure.

## Spec-fidelity statement (filled at close)

Faithful.

The closed item implements hardening design §§6–10 and ADR-0017, and its
dependencies implement ADR-0016 and hardening design §§3–5. The independent
review found no Critical, Important, or Minor divergence attributable to
phases 0, 1, or 2A. Phase 2B planning may begin; implementation remains
unauthorized and future scope remains unchanged.

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
- The semantic supply-chain suite had 73 probes at this record's original
  closure; the 2026-07-23 hardening added 6 (backtick, quote-splice,
  backslash-splice, `\`+newline continuation downloads, an unscanned local
  composite action, and a `publish = false` drift), bringing it to 79. The
  evidence tables above are the historical 73/73 runs and are left as recorded.
  Older counts and hashes remain only where explicitly labeled
  historical/superseded; current identities remain canonical in the
  Operational Task 2 and Task 4 reports.
- Task 5 evidence (session-local truth-reconciliation report, uncommitted;
  the reconciled state is what this record and the live docs now assert).

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

These were slice verdicts, not the parent DoD review. At the Task 5 snapshot,
AC1–AC6 remained unchecked until Task 6 supplied fresh integrated evidence and
independent parent review. The Task 6 closure evidence above now supplies that
review and supersedes the earlier pending state.

At the Task 5 snapshot, this item remained `ongoing`; Task 5 did not run or
claim Task 6's fresh product exit gate, independent parent review, or
phase-unblock decision. That historical boundary remains accurate.
