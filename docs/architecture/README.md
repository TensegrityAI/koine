# Koiné Architecture Wiki

The living map of how Koiné is built: what each module does, how, and why.
Governed by the
[documentation policy](../../.apptlas/policies/documentation-policy.md) —
every page is updated in the same PR as the code it describes (Definition of
Done item 4), so this wiki is trustworthy by construction, not by discipline.

Current state: phase 2A implementation complete; zero-debt hardening active; phase 2B blocked.

## Pages

| Page | Covers | Status |
| --- | --- | --- |
| [overview.md](overview.md) | System shape: planes, crate map, event flow | Current (phase 2A: 7 crates real, rest documented stubs) |
| [koine-domain.md](koine-domain.md) | `Job` aggregate, `JobEvent` taxonomy, `RetryPolicy` | Current (phase 1A) |
| [koine-application.md](koine-application.md) | Driven ports and use cases | Current (phase 1A; phase 2A added the `DispatchSignal`/`WorkerPresence` ports) |
| [koine-store-memory.md](koine-store-memory.md) | In-memory `EventStore`/`Dispatcher` adapters | Current (phase 1A; phase 2A added `NotifySignal`/`NoopPresence`) |
| [koine-store-postgres.md](koine-store-postgres.md) | Postgres `EventStore`/`Dispatcher`/outbox relay, schema, `rebuild_dispatch` | Current (phase 1B; phase 2A added `PgSignal`/`PgPresence` and the `workers` table) |
| [koine-proto.md](koine-proto.md) | `koine.v1` wire contract, codegen, evolution policy | Current (phase 2A) |
| [koine-grpc.md](koine-grpc.md) | `WorkerService` data-plane adapter, auth | Current (phase 2A) |
| [koine-server.md](koine-server.md) | Composition root; `SystemClock`/`UuidV7Ids`; `dev-loop`; `serve` | Current (phase 1B; phase 2A added the authenticated `serve` command) |
| [event-model.md](event-model.md) | Full event taxonomy, envelope, lineage rules | Current (phase 1A) |
| *(remaining per-crate pages)* | One page per crate with real behavior | Born with the phase that builds each crate — phase 3 continues with the control plane |

## How to read this wiki

- Each page answers four things: **what** it does, **how** it is built,
  **why** (linking [ADRs](../adr/INDEX.md)), and its **boundaries**.
- Pages describe what IS. Planned behavior is always marked with its phase.
- Deeper rationale lives in the [design spec](../superpowers/specs/2026-07-16-koine-design.md)
  and the ADRs; pages link rather than restate.
