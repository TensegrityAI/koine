# CLAUDE.md — Koiné living context

**Current phase: 0 — Foundations** (see design spec §6 for all phases).

Active plan: `docs/superpowers/plans/2026-07-18-koine-phase-1a-domain-core.md`.

## Quick orientation

- Start every session by reading `AGENTS.md`; it defines read order and the
  truth hierarchy.
- The design spec (`docs/superpowers/specs/2026-07-16-koine-design.md`) is
  approved — do not relitigate closed decisions (name, Postgres-first, gRPC
  data plane, event taxonomy, dispatch projection strategy).
- Phase 0 exit criterion: full CI green from first push.

## Phase log

- 2026-07-16 — Design spec approved and committed.
- 2026-07-17 — Phase 0 plan written; execution started.
- 2026-07-17 — Phase 0 complete: CI run #1 green on first push (all 7 jobs). Next: roadmap + plans for phases 1-5, then phase 1 (event-sourced core).
- 2026-07-17 — AOL hardened: DoR/DoD gates, rubrics, workflows, instructions, architecture wiki, markdownlint in CI (8 jobs).
- 2026-07-17 — ROADMAP.md + epics for phases 1-5 committed. Next: phase 1 detailed implementation plan (starts with the event-schema ADR), then execution.
