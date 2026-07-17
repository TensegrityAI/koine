# Koiné Roadmap

> A living document: it dictates the *order* of development and what each
> phase must prove before the next begins. Details will mutate as
> implementation teaches us; the sequencing rationale should not. Per-phase
> epics live in `.apptlas/epics/`; the full design is in the
> [design spec](docs/superpowers/specs/2026-07-16-koine-design.md).

**Now:** phase 0 complete (2026-07-17) — workspace, governance, CI. Next up:
phase 1.

## The phases

| Phase | Delivers | Proves (exit criterion) |
| --- | --- | --- |
| **0 — Foundations** ✅ | 11-crate workspace, compiled hexagonal boundaries, AOL governance, CI | Full CI green on first push |
| **1 — Event-sourced core** | `Job` domain + event taxonomy, ports, Postgres + in-memory stores, transactional outbox, dispatch projection; enqueue→lease→ack/fail→retry→park via use cases | All test rings green; projections replay from zero to identical state |
| **2 — Data plane** | **TLA+ model of the lease/delivery protocol**, `koine-proto` v1, gRPC server, leases/heartbeats end-to-end, minimal Python SDK, conformance suite | A real Python worker processes jobs with demonstrable crash recovery; conformance suite passes; TLC verifies the stated protocol properties |
| **3 — Control plane + dashboard v0** | REST + OpenAPI, `koine` CLI (`koine trace`), full observability (OTel + Prometheus), Vite/React/d3 dashboard embedded via rust-embed | OTel traces visible producer→worker cross-language; dashboard live over SSE from a single binary |
| **4 — Agentic** | MCP adapter (enqueue, inspect, operate), rich history projections, dashboard v1 (d3 causal trace graph) | An agent enqueues, inspects history, and operates the broker via MCP |
| **5 — Durable execution** | Checkpoints, signals, human-in-the-loop approvals, **repair & resume** (API + UX) | A job fails at step N, is repaired, and resumes from its last checkpoint |

## Why this order

1. **Core before wire (1→2):** the event model and delivery semantics are the
   product; protocols are adapters over them. Getting leases/retry/outbox
   right against in-memory and Postgres stores is cheap to iterate — the same
   mistakes found after the wire contract ships would be breaking changes.
2. **Model before protocol implementation (TLA+ first in 2):** the bugs that
   kill brokers live in interleavings (crash between lease and ack, late ack
   after expiry, relay restart). Model-checking the protocol before coding it
   is cheaper than discovering interleavings in production — and the model
   becomes the conformance suite's oracle.
3. **Wire before UI (2→3):** the dashboard and CLI consume the control plane;
   the control plane serves data the core already produces. Each layer
   consumes a proven layer.
4. **Agentic before durable (4→5):** MCP is a thin adapter over use cases that
   exist after phase 3; durable execution adds *new* semantics (checkpoints,
   signals) that deserve the last, most careful slot — arriving with the
   operational UX (dashboard, MCP) already in place to exercise them.

## Beyond phase 5 (horizon, unordered)

DAG workflows and batches · additional SDKs (Go, Node, Java) · HA/clustering ·
migration tooling from Faktory/Sidekiq · mdBook docs site · action-pinning and
supply-chain hardening follow-ups (see `.apptlas/backlog/todo/`).

## How phases run

Each phase gets: a detailed implementation plan written **at phase start**
(bite-sized, TDD, like phase 0's — informed by everything learned before it),
executed with per-task independent review, closed through the Definition of
Done with its epic's acceptance criteria verified. Epics in `.apptlas/epics/`
carry each phase's scope, candidate items, risks, and verification strategy.
