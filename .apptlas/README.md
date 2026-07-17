# Agent Operating Layer (`.apptlas/`)

Canonical home for Koiné's agent-operating assets, unifying what kineticrs kept
in `.agents/` + `.github/`. When the apptlas tool's canonical layout lands, this
directory adopts it; until then the kineticrs conventions apply.

- `instructions/` — scoped rules that apply by file pattern (rust style, event
  sourcing, tests, security, proto)
- `backlog/{todo,ongoing,done}` — work items; one file per item, moved between
  dirs as state changes
- `epics/` — multi-item initiatives mapping to design-spec phases
- `policies/` — standing rules (release, review, security)
- `workflows/` — repeatable operating procedures
- `incidents/` — post-mortems and conformance incident reports
- `findings/` — audit findings and architectural debt records
- `skills/` — repo-specific agent skills
