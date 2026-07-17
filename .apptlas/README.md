# Agent Operating Layer (`.apptlas/`)

Canonical home for Koiné's agent-operating assets, unifying what kineticrs kept
in `.agents/` + `.github/`. When the apptlas tool's canonical layout lands, this
directory adopts it; until then the kineticrs conventions apply.

The layer works as a gated lifecycle: items enter the backlog, pass the
[Definition of Ready](policies/definition-of-ready.md) to start, and pass the
[Definition of Done](policies/definition-of-done.md) — with rubric-scored,
non-implementer review — to close. See
[workflows/task-lifecycle.md](workflows/task-lifecycle.md) for the full flow.

- `policies/` — standing rules: definition-of-ready, definition-of-done,
  review, testing, documentation
- `rubrics/` — objective scoring criteria so any reviewer (human or agent)
  reaches the same verdict: code-review, spec-fidelity, docs-quality
- `workflows/` — repeatable procedures: task-lifecycle, adr-workflow
- `instructions/` — scoped rules applied by file pattern: rust-style,
  event-sourcing, testing, docs-style
- `backlog/{todo,ongoing,done}` — work items; one file per item (from
  [backlog/item-template.md](backlog/item-template.md)), moved between dirs
  as state changes
- `epics/` — multi-item initiatives mapping to design-spec phases
- `incidents/` — post-mortems and conformance incident reports
- `findings/` — audit findings and architectural debt records
- `skills/` — repo-specific agent skills (added when real usage patterns
  emerge — deliberately empty for now)
