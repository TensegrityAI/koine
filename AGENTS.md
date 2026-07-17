# AGENTS.md — Koiné

> Operating contract for AI coding agents working in this repository.
> Scope: the whole workspace. Last updated: 2026-07-17.

## 0. Mission

Koiné is an event-sourced, language-agnostic job broker. The history of every job
is the source of truth. We build cathedral-grade foundations: small, verifiable,
reversible changes; no fake completeness; boundaries enforced by the compiler.

## 1. Read order

1. `AGENTS.md` — this contract
2. `CLAUDE.md` — living context and current phase
3. `docs/superpowers/specs/2026-07-16-koine-design.md` — the approved design
4. `docs/adr/INDEX.md` + ADRs relevant to the task
5. `.apptlas/policies/` — the gates your work must pass: definition-of-ready
   before starting, definition-of-done before closing, plus review/testing/
   documentation policies. `.apptlas/instructions/` for rules scoped to the
   files you touch.
6. `.apptlas/backlog/` — active work items
7. `docs/architecture/` — the wiki page(s) for the modules you touch
8. The relevant code, manifests, migrations, proto files

## 2. Truth hierarchy

When sources conflict: **code and manifests → AGENTS.md → ADRs → design spec →
backlog → README/docs.** If code contradicts an accepted ADR, that is
architectural debt: report it, do not copy it as precedent.

## 3. Non-negotiables

- **TDD.** Failing test first, minimal implementation, green, commit.
- **Hexagonal boundaries are crate boundaries.** `koine-domain` has zero internal
  deps and no async/I/O. New inter-crate edges require an ADR.
- **Event log is append-only truth.** No mutation of recorded events, ever.
  State corrections are new events (`JobRepaired`, conflict events).
- **No fake completeness.** No `todo!()`, `unimplemented!()`, or docs claiming
  unwired functionality.
- **Conventional Commits**, enforced by hooks. CI green before merge.
- **Document non-obvious decisions** as ADRs (MADR format, `docs/adr/`;
  triggers and process in `.apptlas/workflows/adr-workflow.md`).
- **The lifecycle gates are not optional.** Work starts only through the
  Definition of Ready and closes only through the Definition of Done —
  including the architecture-wiki update and the spec-fidelity statement
  (`.apptlas/policies/`).

## 4. Layout

- `crates/` — the 11-crate workspace (see design spec §2 for the crate map)
- `sdks/` — worker SDKs (phase 2+), `dashboard/` — embedded SPA (phase 3+)
- `.apptlas/` — agent operating layer: policies, rubrics, workflows,
  instructions, backlog (todo/ongoing/done), epics, incidents, findings, skills
- `docs/architecture/` — the living wiki (what/how/why per module, DoD-enforced)
- `docs/adr/` — architecture decision records; `docs/superpowers/` — specs & plans

## 5. Commands

- `make ci` — everything CI runs except gitleaks, which is CI-only (fmt, clippy -D warnings, test, docs, deny, typos)
- `make test` / `make lint` / `make fmt` — individual rings
- `lefthook install` — git hooks (pre-commit: fmt+typos; pre-push: clippy+test;
  commit-msg: conventional commits)
