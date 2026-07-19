# Handoff: Execute Phase 2A (data-plane server)

For the fresh session picking up Koiné. Read `AGENTS.md` first (truth
hierarchy), then this file, then the plan. Conversation with Marcos is in
Spanish; docs/code in English. He welcomes honest pushback and ambitious
targets.

## State (2026-07-19)

- Repo: `github.com/TensegrityAI/koine`, main @ `6743e55`, CI 8/8 green
  (incl. ring-3 testcontainers on the runner). Working tree clean.
- **Phases 0, 1 (A+B) COMPLETE.** 75 workspace tests. Dev-loop product
  exercise proven (`cargo run -p koine-server -- dev-loop` vs live Postgres).
- **Next: execute** `docs/superpowers/plans/2026-07-19-koine-phase-2a-data-plane-server.md`
  (10 tasks: ADRs 0013–0015 → TLA+/TLC gate → proto v1 → signal/presence
  ports → gRPC service+auth → wire tests → serve → carryover fold-in →
  ring-3 e2e → closeout). Phase 2B (Python SDK, ring-4 conformance,
  benchmarks, crates.io) gets its own plan AFTER 2A executes.
- Marcos has approved subagent-driven execution as the standing mode.

## Bootstrap (exact)

```bash
git checkout -b phase-2a-data-plane-server
cat >> .superpowers/sdd/progress.md <<'EOF'

== PHASE 2A (plan 2026-07-19-koine-phase-2a-data-plane-server.md, branch phase-2a-data-plane-server) ==
EOF
BASE=$(git rev-parse HEAD)
/home/nexus/.claude/plugins/cache/claude-plugins-official/superpowers/6.1.1/skills/subagent-driven-development/scripts/task-brief docs/superpowers/plans/2026-07-19-koine-phase-2a-data-plane-server.md 1
```

Then invoke `superpowers:subagent-driven-development` and run the loop.

## Execution protocol (distilled from 3 phases; it works — keep it)

- Per task: record BASE → `task-brief N` → dispatch implementer → on DONE:
  `review-package BASE HEAD` → dispatch reviewer with brief+report+diff
  paths + verbatim binding constraints + 2–4 adversarial PROBES (temp
  tests, deleted after; this is where the real bugs surfaced) → fix
  subagent for Critical/Important → re-review via SendMessage to the SAME
  reviewer → ledger line in `.superpowers/sdd/progress.md` (gitignored
  scratch; survives on this machine — read the old sections for history).
- Models: haiku implementers for code-complete briefs; sonnet for
  structural-spec briefs (tasks 5–7, 9, 10) and ALL reviewers; **fable for
  the final whole-branch review only**, with the accumulated Minor list for
  triage. Reviewers verify claims EMPIRICALLY (run, mutate, revert) — never
  trust reports.
- Merge flow: final review READY → fix riders → merge `--no-ff` to main →
  push → poll CI via
  `curl -fsS "https://api.github.com/repos/TensegrityAI/koine/actions/runs?head_sha=$(git rev-parse HEAD)&per_page=1"`
  (background loop, then per-job listing) → closeout records → update
  memory + report to Marcos.

## Battle-tested conventions (violations = review findings)

- Honest deviations: two phase-1 reports were corrected for false "No
  Deviations" claims. If an implementer touches ANY file outside its
  brief's list, the report must say so prominently.
- **Never change production semantics to satisfy a failing test** — a
  defective PLAN test caused the extend_lease accumulating-semantics drift
  (caught, reverted). Fix the test or escalate; the plan can be wrong.
- Failures are side-effect-free: rolled-back tx / failed append leaves
  NOTHING (two real bugs found here — phantom stream, fold-rejected
  persist). Probe failure paths.
- Plain `async fn` in trait impls; no `manual_async_fn` allows. Integration
  test files open with `#![allow(clippy::expect_used)]` + rationale comment
  (clippy.toml only exempts `#[test]` fns). `Duration::from_mins` etc. —
  pedantic `duration_suboptimal_units` is enforced.
- No security-advisory ignores in deny.toml — bump versions instead
  (precedent: testcontainers 0.27). Cargo.lock MUST be committed with any
  manifest change; verify with `--locked`.
- Mutation-check trap: proptest writes `.proptest-regressions` files that
  make later runs deterministic — delete between mutation trials.
- Session limits mid-flight: on resume, FIRST check `git status` (a killed
  reviewer may leave experimental edits), then resume the agent via
  SendMessage with its agentId from the ledger/transcript.

## Plan-specific watchpoints for 2A

- Task 2 TLC: if TLC finds a counterexample, STOP and escalate to Marcos —
  it back-propagates as a phase-1 fidelity finding (epic risk).
- Task 3: tonic 0.14 codegen crate naming (`tonic-prost-build`) may need
  resolution against reality — resolve, pin, record exact versions.
- Task 5 auth: constant-time compare, `UNAUTHENTICATED` without detail;
  never claim native TLS.
- Task 8 prunes koine-server's unused deps AFTER task 7 wires grpc — order
  matters.
- The wiki-freshness rule bit us in 1B (wrong sweep description shipped to
  final review): any task touching application/domain surface updates the
  page in the SAME task or discloses why not.

## Open items / context

- `.apptlas/backlog/todo/phase-2-carryover-hardening.md`: 2A closes three
  ACs (task 8); pool-knob + relay/sink deadlock note stay for 2B/3.
- crates.io publication deferred to 2B (after
  `manifest-cleanup-workspace-deps`).
- Auth v1 scope was maintainer-ratified 2026-07-18 (recorded in epics).
- Memory file `project-koine.md` (auto-memory) is current as of this
  handoff; update it at 2A close.
