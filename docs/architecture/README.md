# Koiné Architecture Wiki

The living map of how Koiné is built: what each module does, how, and why.
Governed by the
[documentation policy](../../.apptlas/policies/documentation-policy.md) —
every page is updated in the same PR as the code it describes (Definition of
Done item 4), so this wiki is trustworthy by construction, not by discipline.

## Pages

| Page | Covers | Status |
| --- | --- | --- |
| [overview.md](overview.md) | System shape: planes, crate map, event flow | Current (phase 0: crates are documented stubs) |
| *(per-crate pages)* | One page per crate with real behavior | Born with the phase that builds each crate — phase 1 starts with `koine-domain`, `koine-application`, the stores |

## How to read this wiki

- Each page answers four things: **what** it does, **how** it is built,
  **why** (linking [ADRs](../adr/INDEX.md)), and its **boundaries**.
- Pages describe what IS. Planned behavior is always marked with its phase.
- Deeper rationale lives in the [design spec](../superpowers/specs/2026-07-16-koine-design.md)
  and the ADRs; pages link rather than restate.
