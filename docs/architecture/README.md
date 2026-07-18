# Koiné Architecture Wiki

The living map of how Koiné is built: what each module does, how, and why.
Governed by the
[documentation policy](../../.apptlas/policies/documentation-policy.md) —
every page is updated in the same PR as the code it describes (Definition of
Done item 4), so this wiki is trustworthy by construction, not by discipline.

## Pages

| Page | Covers | Status |
| --- | --- | --- |
| [overview.md](overview.md) | System shape: planes, crate map, event flow | Current (phase 1B: 5 crates real, rest documented stubs) |
| [koine-domain.md](koine-domain.md) | `Job` aggregate, `JobEvent` taxonomy, `RetryPolicy` | Current (phase 1A) |
| [koine-application.md](koine-application.md) | Driven ports and use cases | Current (phase 1A) |
| [koine-store-memory.md](koine-store-memory.md) | In-memory `EventStore`/`Dispatcher` adapters | Current (phase 1A) |
| [koine-store-postgres.md](koine-store-postgres.md) | Postgres `EventStore`/`Dispatcher`/outbox relay, schema, `rebuild_dispatch` | Current (phase 1B) |
| [koine-server.md](koine-server.md) | Composition root; `SystemClock`/`UuidV7Ids`; `dev-loop` | Current (phase 1B) |
| [event-model.md](event-model.md) | Full event taxonomy, envelope, lineage rules | Current (phase 1A) |
| *(remaining per-crate pages)* | One page per crate with real behavior | Born with the phase that builds each crate — phase 2 continues with the data plane |

## How to read this wiki

- Each page answers four things: **what** it does, **how** it is built,
  **why** (linking [ADRs](../adr/INDEX.md)), and its **boundaries**.
- Pages describe what IS. Planned behavior is always marked with its phase.
- Deeper rationale lives in the [design spec](../superpowers/specs/2026-07-16-koine-design.md)
  and the ADRs; pages link rather than restate.
