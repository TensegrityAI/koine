# Koiné Roadmap

> A living document: it dictates the *order* of development and what each
> phase must prove before the next begins. Details may change as implementation
> teaches us; the sequencing rationale does not. Per-phase epics live in
> `.apptlas/epics/`; the full design is in the
> [design spec](docs/superpowers/specs/2026-07-16-koine-design.md).

**Now:** Phase 2A complete and hardened — next: phase 2B planning (not started). Phase 2B implementation is not authorized; no phase-2B item or plan exists.

## The phases

| Phase | State | Delivers | Proves (exit criterion) |
| --- | --- | --- | --- |
| **0 — Foundations** | Complete | 11-crate workspace, compiled hexagonal boundaries, AOL governance, CI | Full CI green on first push |
| **1 — Event-sourced core** | Complete | `Job` domain and event taxonomy, ports, Postgres and in-memory stores, transactional outbox, dispatch projection; enqueue → lease → ack/fail → retry → park | Test rings 1–3 green; projections replay from zero to identical state |
| **2 — Data plane** | 2A complete and hardened; 2B planning not started | **2A:** TLA+ lease model, `koine-proto` v1, authenticated gRPC server, leases/heartbeats/recovery. **2B future scope:** Python SDK, conformance, SDK demo, benchmarks, publication decision, test-support dedup | A real Python worker processes jobs with demonstrable crash recovery; conformance passes; TLC verifies the stated protocol properties |
| **3 — Control plane + dashboard v0** | Planned | REST + OpenAPI, `koine` CLI (`koine trace`), full observability (OTel + Prometheus), Vite/React/d3 dashboard embedded via rust-embed | OTel traces visible producer → worker cross-language; dashboard live over SSE from a single binary |
| **4 — Agentic** | Planned | MCP adapter (enqueue, inspect, operate), rich history projections, dashboard v1 (d3 causal trace graph) | An agent enqueues, inspects history, and operates the broker via MCP |
| **5 — Durable execution** | Planned | Checkpoints, signals, human-in-the-loop approvals, **repair & resume** (API + UX) | A job fails at step N, is repaired, and resumes from its last checkpoint |

## Phase 2 detail

Phase 2A provides the event-sourced stores and dispatch path, the
authenticated `koine.v1` server-streaming worker API, heartbeat/expiry fencing,
one shared Postgres notification listener, bounded database resources, and the
TLC-checked lease model. Its zero-debt exit gate is closed. Phase 2B planning
is authorized but not started, and phase 2B implementation is not authorized.

The legitimate phase-2B scope remains:

- minimal Python SDK;
- language-agnostic ring-4 conformance suite;
- scripted crash-recovery demo against that SDK;
- baseline benchmarks, including the server-streaming versus bidirectional
  revisit;
- crates.io publication decision and package verification (all crates remain
  `publish = false` today);
- deduplication of the real Postgres/gRPC test-support runtime helpers.

## Why this order

1. **Core before wire (1 → 2):** the event model and delivery semantics are the
   product; protocols are adapters over them. Getting leases/retry/outbox
   right against in-memory and Postgres stores is cheap to iterate — the same
   mistakes found after the wire contract ships would be breaking changes.
2. **Model before protocol implementation (TLA+ first in 2):** the bugs that
   kill brokers live in interleavings (crash between lease and ack, heartbeat
   against expiry, late ack after expiry). The model is checked before and
   alongside implementation; in phase 2B it will become the future conformance
   suite's oracle.
3. **Wire before UI (2 → 3):** the dashboard and CLI consume the control plane;
   the control plane serves data the core already produces. Each layer
   consumes a proven layer.
4. **Agentic before durable (4 → 5):** MCP is a thin adapter over use cases that
   exist after phase 3; durable execution adds new semantics (checkpoints,
   signals) that arrive with the operational UX already in place to exercise
   them.

## Beyond phase 5 (horizon, unordered)

DAG workflows and batches · additional SDKs (Go, Node, Java) · HA/clustering ·
migration tooling from Faktory/Sidekiq · mdBook docs site (go/no-go decided
inside phase 3).

## How phases run

Each phase gets a detailed implementation plan written **at phase start**,
executed with per-task independent review, and closed through the Definition of
Done with its epic's acceptance criteria verified. Epics in `.apptlas/epics/`
carry each phase's scope, candidate items, risks, and verification strategy.
