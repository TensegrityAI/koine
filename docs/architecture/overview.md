# System Overview

## What Koiné is

Koiné is an event-sourced, language-agnostic job broker. The history of every
job is the source of truth (ADR 0004): all state derives from an append-only
event log, which makes traceability, replay, and repair-&-resume structural
properties rather than features bolted on.

**Status: phase 2A — phase 1 complete, data plane server delivered.** The
workspace, boundaries, and governance below exist; `koine-domain`,
`koine-application`, `koine-store-memory`, `koine-store-postgres`,
`koine-proto`, `koine-grpc`, and `koine-server` now have real behavior (see
the crate table and their pages below). The data plane is a real,
authenticated `gRPC` server today (`koine-server serve`), not only a design.
Remaining crates (the control plane: `koine-http`, `koine-cli`, `koine-mcp`,
plus `koine-observability`) are still documented stubs; behavior arrives per
phase (design spec §6).

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
        │ gRPC: server-stream Fetch  │
        │  + unary Start/Ack/Beat    │
        ▼                             ▼
   Python worker                 Go/Node/… worker
```

- **Data plane** (workers; high volume, long-lived): gRPC with a versioned
  proto contract (`koine.v1`, `koine-proto`) — SDKs are generated, not
  reverse-engineered. Delivered in phase 2A as `koine-grpc` +
  `koine-server serve`: a long-lived server-streaming `Fetch` plus unary
  `Start`/`Succeed`/`Fail`/`Heartbeat` — this diagram originally showed a
  full bidi-stream; ADR 0013 records the divergence and its revisit
  trigger (phase-2B benchmarks). Authenticated per ADR 0014 (shared bearer
  token; TLS is proxy-terminated, not native); wakeup is push-based via
  Postgres `LISTEN`/`NOTIFY` with a polling backstop (`DispatchSignal`,
  see [koine-application.md](koine-application.md)).
- **Control plane** (producers, operators, agents): REST + OpenAPI, the
  operator CLI, and an MCP adapter so agents operate the broker first-class
  — still phase 3+ documented stubs.

## The crates

The hexagon is compiled: boundaries are crate boundaries, and the dependency
graph forbids illegal imports (ADR 0003). Direction: domain ← application ←
adapters ← server.

Internal crate identities live once in root `[workspace.dependencies]` with
both version and path; member manifests inherit those entries without changing
dependency kind, rename, features, default features, target, or optionality.
Every crate is `publish = false` while phase 2B remains blocked. For the seven
implemented crates, package-file inspection includes the build sources and
runtime assets plus regular `LICENSE` and `NOTICE` copies. The supply-chain
gate compares those copies byte-for-byte with the workspace originals and
rejects missing files or symlinks; this verifies file boundaries without
authorizing publication.

| Crate | Layer | Role (stubs marked with the phase real behavior arrives) |
| --- | --- | --- |
| `koine-domain` | Domain | Aggregates, events, state machines. No async, no I/O — see [koine-domain.md](koine-domain.md) |
| `koine-application` | Application | Use cases + driven ports (`EventStore`, `Dispatcher`, `Clock`, `IdGenerator`, `DispatchSignal`, `WorkerPresence`) — see [koine-application.md](koine-application.md) |
| `koine-store-postgres` | Driven | Event store, transactional outbox, dispatch projection, `LISTEN`/`NOTIFY` signal, worker presence — see [koine-store-postgres.md](koine-store-postgres.md) |
| `koine-store-memory` | Driven | Full in-memory port implementations for tests — see [koine-store-memory.md](koine-store-memory.md) |
| `koine-proto` | Contract | `koine.v1` versioned protobuf wire contract, standalone — see [koine-proto.md](koine-proto.md) |
| `koine-grpc` | Driving | Data plane adapter: authenticated `WorkerService` — see [koine-grpc.md](koine-grpc.md) |
| `koine-http` | Driving | Control plane REST + embedded dashboard (phase 3) |
| `koine-observability` | Infra | OTel/Prometheus init (phase 3) |
| `koine-cli` | Binary | Operator CLI (phase 3) |
| `koine-mcp` | Driving | Agent control plane (phase 4) |
| `koine-server` | Binary | Composition root; `dev-loop`; authenticated `serve` (phase 2A) — grows with each phase — see [koine-server.md](koine-server.md) |

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
- `koine.v1` wire contract: server-streaming `Fetch` + unary acks, JSON
  payloads, additive-only evolution — ADR 0013
- Worker auth v1: single shared bearer token, proxy-terminated TLS — ADR 0014
- Worker presence is ephemeral infrastructure state, not an event-sourced
  aggregate — ADR 0015
- Full index: [docs/adr/INDEX.md](../adr/INDEX.md)

## Boundaries with the outside

- **Postgres** is the only required runtime dependency (ADR 0005).
- Repository-owned Compose and testcontainers consumers use the reviewed
  Postgres 17 image digest; the supply-chain gate rejects tag-only and
  wrong-digest substitutions (ADR 0017).
- Workers in any language speak the `koine-proto` contract, enforced today
  by a real server (`koine-grpc` + `koine-server serve`); a ring-4
  conformance suite against a generated SDK (phase 2B) is the polyglot
  promise's compatibility gate, not yet built.
- The dashboard (phase 3) is a static SPA embedded in `koine-server` — the
  deploy story stays single-binary (ADR 0009).
