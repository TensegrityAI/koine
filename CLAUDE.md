# CLAUDE.md — Koiné living context

**Current phase:** Phase 2A complete and hardened — next: phase 2B planning (not started). Phase 2B implementation is not authorized. (See design spec §6 for all phases.)

**Active plan:** none. The most recently completed plan is
[`2026-07-21-koine-operational-closure.md`](docs/superpowers/plans/2026-07-21-koine-operational-closure.md).
Phase 2B planning is authorized but has not started; no phase-2B item or plan
exists yet.

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
- 2026-07-18 — Phase 1A complete: event-sourced domain core green on rings 1–2. Next: phase 1B plan (Postgres store, outbox, dispatch projection, ring 3).
- 2026-07-18 — Phase 1B complete: Postgres store, outbox relay, ring 3, dev-loop. PHASE 1 COMPLETE. Next: phase 2 plan (TLA+ model first — epic item 1).
- 2026-07-19 — Phase 2A implementation checkpoint: TLA+ lease-protocol model TLC-checked in CI (`tla` job); `koine.v1` wire contract (`koine-proto`); authenticated `gRPC` data-plane server (`koine-grpc` + `koine-server serve`) with `DispatchSignal`/`WorkerPresence` ports, Postgres `LISTEN`/`NOTIFY` wakeup, worker presence table; crash recovery proven over a real socket + real Postgres. Epic items 1–6 delivered. At this checkpoint carryover AC4 and zero-debt hardening were still open, so this was implementation completion rather than the operational exit gate.
- 2026-07-21 — Atomic lease retirement and Postgres resource hardening closed: heartbeat and expiry now serialize on the live grant; one listener is shared across Fetch waits; the operational pool is bounded; presence latency is best-effort and bounded.
- 2026-07-22 — Operational Tasks 1–5 complete: immutable executable inputs, the fail-closed 73-probe semantic supply-chain gate, vendored protobuf compilation, immutable Postgres consumers, centralized manifest edges, non-publishable crates, package-file boundaries, and public/lifecycle truth are reconciled. Task 6 remains blocked on fresh exit evidence and independent review.
- 2026-07-22 — Phase 2A zero-debt hardening closed after fresh formal, CI,
  Postgres, gRPC, product, residue, package, and supply-chain evidence plus an
  independent dual-verdict review with no attributable findings. Phase 2B
  planning is next but not started; phase 2B implementation is not authorized.
- 2026-07-23 — Post-hardening polish (independent multi-agent re-review of the
  51-commit hardening batch): all four domains verdicted sound/truthful, no
  Critical/Important defects in the merged code. Follow-ups applied: presence
  drop-counter + de-flaked e2e; listener-reconnect logging; bounded fence test;
  supply-chain gate rewritten to a command-position shell tokenizer (closes
  backtick/quote-splice/backslash-splice/continuation download bypasses),
  local composite actions forbidden, `publish = false` enforced, `_archive`
  excluded (73→79 probes); corrected the closure record's uncommitted
  "durable review" citations; TLA drift-rule attributed to the dispatcher
  layer. Phase 2B planning still next, still not started, implementation not
  authorized.
