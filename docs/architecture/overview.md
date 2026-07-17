# System Overview

## What Koiné is

Koiné is an event-sourced, language-agnostic job broker. The history of every
job is the source of truth (ADR 0004): all state derives from an append-only
event log, which makes traceability, replay, and repair-&-resume structural
properties rather than features bolted on.

**Status: phase 0.** The workspace, boundaries, and governance below exist;
all crates are documented stubs. Behavior arrives per phase (design spec §6).

## How it is shaped

Two planes over a strict hexagonal core (ADR 0007):

```text
                    PRODUCERS / OPERATORS / AGENTS
                        │REST (OpenAPI)   │MCP    │CLI
                   ┌────▼─────────────────▼───────▼────┐
   CONTROL PLANE   │        Driving adapters            │
                   ├────────────────────────────────────┤
                   │   APPLICATION (use cases, ports)   │
                   │   DOMAIN (Job, Queue, Lease, …)    │
                   ├────────────────────────────────────┤
                   │        Driven adapters             │
                   │  EventStore │ Projections │ Outbox │
                   └──────┬─────────────────────────────┘
   DATA PLANE             │ Postgres (event log = truth)
        ┌─────────────────┴──────────┐
        │  gRPC bidi-stream           │
        ▼                             ▼
   Python worker                 Go/Node/… worker
```

- **Data plane** (workers; high volume, long-lived): gRPC with a versioned
  proto contract — SDKs are generated, not reverse-engineered.
- **Control plane** (producers, operators, agents): REST + OpenAPI, the
  operator CLI, and an MCP adapter so agents operate the broker first-class.

## The crates

The hexagon is compiled: boundaries are crate boundaries, and the dependency
graph forbids illegal imports (ADR 0003). Direction: domain ← application ←
adapters ← server.

| Crate | Layer | Role (all stubs today; behavior arrives at the phase shown) |
| --- | --- | --- |
| `koine-domain` | Domain | Aggregates, events, state machines. No async, no I/O (phase 1) |
| `koine-application` | Application | Use cases + driven ports (`EventStore`, `OutboxRelay`, `ProjectionStore`, `LeaseManager`, `Clock`, `IdGenerator`) (phase 1) |
| `koine-store-postgres` | Driven | Event store, transactional outbox, projections (phase 1) |
| `koine-store-memory` | Driven | Full in-memory port implementations for tests (phase 1) |
| `koine-proto` | Contract | Versioned protobuf wire contract, standalone (phase 2) |
| `koine-grpc` | Driving | Data plane adapter (phase 2) |
| `koine-http` | Driving | Control plane REST + embedded dashboard (phase 3) |
| `koine-observability` | Infra | OTel/Prometheus init (phase 3) |
| `koine-cli` | Binary | Operator CLI (phase 3) |
| `koine-mcp` | Driving | Agent control plane (phase 4) |
| `koine-server` | Binary | Composition root — grows with each phase (from phase 1) |

## Why: the load-bearing decisions

- Event log as single source of truth, with durable-execution event kinds
  reserved from day one — ADR 0004
- Postgres event store behind a port; in-memory adapter keeps the port
  honest — ADR 0005
- Hot path: dispatch projection updated in the append transaction
  (`SKIP LOCKED` fetch); everything else async via transactional outbox —
  ADR 0006
- At-least-once delivery with leases and heartbeats; late acks become
  conflict events, never lost information — ADR 0008
- Full index: [docs/adr/INDEX.md](../adr/INDEX.md)

## Boundaries with the outside

- **Postgres** is the only required runtime dependency (ADR 0005).
- Workers in any language speak the `koine-proto` contract; a conformance
  suite (phase 2) is the polyglot promise made verifiable.
- The dashboard (phase 3) is a static SPA embedded in `koine-server` — the
  deploy story stays single-binary (ADR 0009).
