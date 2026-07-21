# Koiné — Design Specification

- **Date:** 2026-07-16
- **Authors:** Marcos Saez (Kael) & Claude
- **Status:** Approved design, pending implementation plan
- **Supersedes:** the 2025 NEXUS draft and PoC (now in `_archive/`)
- **Amended:** 2026-07-21 by the approved
  [phase-2A zero-debt hardening design](2026-07-21-koine-phase-2a-zero-debt-hardening-design.md),
  ADR-0016, and ADR-0017. Those records refine §3 delivery semantics, §4 CI
  and testing, and the phase-2 closure gate in §6.

---

## 1. Identity and thesis

**Koiné** (κοινή, "the common [language]") is an event-sourced, language-agnostic job broker
written in Rust. The name is the thesis: the koiné was the shared language that emerged so
speakers of many dialects could work together — Koiné is the common language between
programming languages for background work.

**Core thesis:** *the history of every job is the source of truth, not a byproduct.*
From that single architectural commitment, three capabilities follow that no open-source
broker currently combines:

1. **Total traceability** — what happened, why, in what order, with what context; the causal
   chain of every job is queryable and replayable, across languages.
2. **Repair & resume (durable execution)** — journaled side effects (checkpoints), signals,
   human-in-the-loop approvals. A failed job is not "retried and hope"; it is inspected,
   repaired, and continued from its last checkpoint with full prior history preserved.
3. **Agent-native operation** — the control plane is consumable by agents via MCP: agents can
   enqueue, inspect history, and operate the broker as first-class clients.

### Why this matters in 2026

Classic brokers (Sidekiq, Faktory, Celery) model a job as a deterministic function retried
whole on failure. Agentic workloads break that model: a job may run 40 minutes, make 15
LLM/API calls, and fail at step 14. Re-running it whole is expensive, non-deterministic, and
unsafe (duplicated side effects). Durable-execution systems (Temporal, Restate, Inngest)
solve this but are heavyweight, SDK-invasive, or VC/SaaS-centric — and none is a simple
polyglot *broker* you can push a job to from any language.

**The market gap Koiné targets:** an open, self-hostable, polyglot broker that scales from
"normal job queue" to "durable agent execution" with a single event model — where the classic
job is simply the degenerate case (a job with zero checkpoints).

### Positioning vs. prior art

| | Sidekiq/Faktory | Temporal/Restate | Koiné |
|---|---|---|---|
| Polyglot | Protocol reimplemented per client | SDK-invasive workflows | gRPC contract + generated SDKs + conformance suite |
| Failure story | Dead set, good luck | Deterministic replay | Event history + repair & resume from checkpoint |
| Traceability | Logs, maybe | Internal history | Event log as source of truth, OTel end-to-end |
| Agent operation | None | None | MCP control plane |
| Enterprise features | Paid | SaaS tiers | Free, Apache-2.0 |

### Identity decisions

- **License:** Apache-2.0 (explicit patent grant matters for enterprise adoption).
- **Canonical host:** GitHub.
- **Language:** Rust (server), TypeScript (dashboard), Python first worker SDK.
- The name NEXUS was dropped due to hard collision with Sonatype Nexus; "Rosetta" was
  considered and dropped (Apple Rosetta, Rosetta Stone trademark). `koine` is free on
  crates.io as of 2026-07-16.

---

## 2. Architecture overview

Strict hexagonal architecture with two separated planes:

```
                    PRODUCERS / OPERATORS / AGENTS
                        │REST (OpenAPI)   │MCP    │CLI
                   ┌────▼─────────────────▼───────▼────┐
   CONTROL PLANE   │        Driving adapters            │
                   ├────────────────────────────────────┤
                   │   APPLICATION (use cases, sagas)   │
                   │   DOMAIN (Job, Queue, Lease, …)    │
                   ├────────────────────────────────────┤
                   │        Driven ports                │
                   │  EventStore │ Projections │ Outbox │
                   └──────┬─────────────────────────────┘
   DATA PLANE             │ Postgres (event log = truth)
        ┌─────────────────┴──────────┐
        │  gRPC bidi-stream           │
        ▼                             ▼
   Python worker                 Go/Node/Java worker…
   (SDK generated from proto)
```

- **Control plane** (casual clients, low volume): REST with OpenAPI + MCP adapter + CLI.
- **Data plane** (workers, high volume, long-lived, bidirectional): gRPC as the canonical
  protocol. The `.proto` files are a versioned first-class contract; official codegen gives
  every language a typed client without reverse-engineering a wire protocol (the exact
  failure mode of Faktory that motivated faktory-tools).

### Workspace layout — compilation-enforced hexagon

Cargo workspace where hexagonal boundaries are crate boundaries; the dependency graph makes
architecture violations impossible to compile (`koine-domain` cannot depend on sqlx).

| Crate | Layer | Contents |
|---|---|---|
| `koine-domain` | Domain | Aggregates (`Job`, `Queue`, `WorkerRegistration`), domain events, state machines, retry policy. **No async, no I/O, no infra deps.** |
| `koine-application` | Application | Use cases (command/query handlers) and the **ports** (traits): `EventStore`, `OutboxRelay`, `ProjectionStore`, `LeaseManager`, `Clock`, `IdGenerator`. (Saga orchestrator lands here later, with the future workflow spec.) |
| `koine-proto` | Contract | Versioned `.proto` + tonic codegen. Source of all worker SDKs. |
| `koine-store-postgres` | Driven | Event store (append with optimistic concurrency, snapshots), transactional outbox, projections — sqlx. |
| `koine-store-memory` | Driven | Complete in-memory adapter for tests; guarantees ports are not Postgres-coupled. |
| `koine-grpc` | Driving | Data plane: fetch stream, ack/fail, heartbeats, checkpoints. |
| `koine-http` | Driving | Control plane REST with OpenAPI (utoipa); serves the embedded dashboard. |
| `koine-mcp` | Driving | Agent control plane (rmcp; API-key auth + Origin/DNS-rebinding validation). |
| `koine-observability` | Infra | OTel/tracing/Prometheus init. |
| `koine-server` | Binary | Composition root: config, DI, startup, graceful shutdown. |
| `koine-cli` | Binary | Operator CLI (`koine trace <job>`, queue ops). |

Worker SDKs live in `sdks/` (Python first), generated from `koine-proto` plus a thin
idiomatic layer per language. The dashboard lives in `dashboard/` (see §5).

### Provenance (audited 2026-07-16)

Fresh workspace with surgical cherry-picking — explicitly **not** copy-and-clean:

- From **kineticrs** (as reference to reimplement generically, since its `EventStore` trait
  is concretely bound to its `TodoEvent` aggregate): Postgres event store + snapshot logic,
  saga orchestrator (near-liftable), projection runner, observability wiring, migration
  schema structure, generic ADRs, `.agents/` skeleton and methodology.
- From **a-simple-todo-app**: MCP adapter patterns (rmcp usage, auth middleware with Origin
  validation, live OpenAPI/schema exposed as MCP resources), TS hexagonal frontend
  conventions (ports/adapters), ADR/plans lifecycle structure.
- Known kineticrs gaps that Koiné fixes at birth: no transactional outbox (dual-write),
  single-crate (no compiled boundaries), floating `stable` toolchain pin, integration tests
  that don't run real migrations.

---

## 3. The core: event model and delivery semantics

### Job lifecycle state machine

Every transition is an immutable event in the log:

```
enqueued ──▶ scheduled ──▶ leased ──▶ running ──▶ succeeded
   │            ▲            │           │
   │            │            ▼           ▼
   └─(cron/     └──retry──  lease_expired  failed ──▶ parked (dead)
      delay)       (backoff)                  │           │
                                              ▼           ▼
                                          checkpointed  repaired ──▶ re-enqueued
                                          suspended ◀── signal/approval
```

Property to be enforced by proptest: no event sequence can reach an illegal state.

### Event taxonomy

Designed complete from day one; implemented in phases. The v1 schema reserves the durable
execution event kinds so later phases require no architectural change.

- **Core lifecycle (v1):** `JobEnqueued` (payload, queue, priority, retry policy,
  `correlation_id`, `causation_id`, W3C traceparent), `JobLeased`, `JobStarted`,
  `JobSucceeded`, `JobFailed` (structured error: type, message, stacktrace, retryable flag),
  `LeaseExpired`, `JobRetryScheduled`, `JobParked`, `JobCancelled`, `JobStalled`.
- **Durable execution (phase 5, schema from v1):** `CheckpointRecorded` (journaled side
  effect result — the agent's memory), `SignalReceived`, `ApprovalRequested`,
  `ApprovalGranted`/`ApprovalDenied`, `JobSuspended`, `JobResumed`.
- **Repair (the killer feature):** `JobRepaired` — an operator (human, or agent via MCP)
  edits payload/state and re-enqueues *preserving all prior history*. The job is not
  retried; it is repaired and continues from its last checkpoint.

### Delivery semantics: at-least-once with leases

- A worker does not "receive" a job; it acquires a **lease** with a TTL, renewed by
  heartbeats over the gRPC stream.
- Worker dies → lease expires → job becomes eligible again; `LeaseExpired` records exactly
  what happened. No silent loss, ever.
- A late ACK after lease expiry is recorded as an explicit conflict event — information is
  never discarded.
- Retries: exponential backoff + jitter per the retry policy declared at enqueue. Exhausted
  attempts → `parked`, with full history, awaiting repair.
- Heartbeats and progress % are ephemeral (outside the log) — but threshold crossings are
  events (`JobStalled`).

### The hot path — two projection tiers

Where event sourcing usually dies on throughput, Koiné splits consistency by criticality:

1. **Dispatch projection (synchronous):** the `dispatch_queue` table is updated **in the
   same transaction** as the event append. Worker fetch does
   `SELECT … FOR UPDATE SKIP LOCKED` on it — the most battle-tested Postgres job-queue
   pattern in existence. Strong consistency exactly where it matters.
2. **Read projections (asynchronous):** history, metrics, dashboard views — fed via the
   transactional outbox (event + outbox row in one tx; relay with persistent positions).
   Rebuildable from the log at any time.

### End-to-end traceability

Every event carries `correlation_id`, `causation_id`, and W3C trace context — the producer's
OTel trace continues inside the Python worker and back. `koine trace <job_id>` renders the
full causal history, cross-language.

---

## 4. Governance, quality, and testing

### Agent Operating Layer under `.apptlas/`

Kineticrs methodology, unified (`.github` content merged):
`.apptlas/{instructions,backlog/{todo,ongoing,done},epics,policies,workflows,incidents,findings,skills}`.
When the latest apptlas tool version is available on this machine, its exact layout is
adopted; until then the known kineticrs convention applies. Root contract files: `AGENTS.md`
(operating contract, truth hierarchy: code → AGENTS.md → ADRs), `CLAUDE.md` (living context),
`docs/adr/` in MADR format.

Founding ADRs (from this design conversation):

| ADR | Decision |
|---|---|
| 0001 | Identity: Koiné, thesis, name rationale |
| 0002 | Apache-2.0, GitHub canonical |
| 0003 | Multi-crate workspace; hexagonal boundaries as compilation boundaries |
| 0004 | Event log as single source of truth + durable-execution semantics |
| 0005 | Postgres event store behind a port; in-memory adapter for tests |
| 0006 | Synchronous dispatch projection + transactional outbox for async projections |
| 0007 | gRPC canonical data plane; REST + MCP control plane |
| 0008 | At-least-once delivery with leases and heartbeats |
| 0009 | Dashboard: Vite+React SPA, OpenAPI-generated client, embedded via rust-embed |

### Repo hygiene

Rust edition 2024; `rust-toolchain.toml` pinned to an **exact** version; rustfmt; clippy
`-D warnings`; cargo-deny; typos; commitlint; lefthook with **no external binary
dependencies**; `.editorconfig`; CODEOWNERS; SECURITY.md; CONTRIBUTING.md.

### CI (GitHub Actions)

fmt, clippy, test (unit + integration), cargo-deny, typos, gitleaks, docs build, dashboard
lint/test/build. Only jobs that pass from commit one — no CI coupled to absent tooling.

### Testing — three rings plus one

1. **Domain:** pure unit tests + **proptest over the job state machine** (no event sequence
   reaches an illegal state).
2. **Application:** use cases against `koine-store-memory` — fast, no Docker.
3. **Integration:** testcontainers Postgres running the **real migrations** via
   `sqlx::migrate!`. Scenarios: worker crash → lease expiry → retry; late ack conflict;
   projection replay from zero.
4. **Protocol conformance suite:** a language-agnostic harness any SDK must pass
   (fetch/ack/fail/heartbeat/checkpoint against a real broker). This turns
   "language-agnostic" from promise into verifiable contract.

---

## 5. Dashboard

- **Stack:** Vite + React + TypeScript SPA in `dashboard/`; TS hexagonal structure
  (ports/adapters, patterns from a-simple-todo-app without its code); API client generated
  from the OpenAPI spec — the dashboard is deliberately the first "foreign-language"
  consumer of the control plane, dogfooding the polyglot promise.
- **Visualization:** d3.js for bespoke, data-dense visualizations (queue flows, event
  timelines, causal trace graphs) — not a generic chart library, to retain full control.
- **Design language:** professional minimalist, Palantir/Blueprint sensibility — dark,
  data-dense, precise typography, restrained motion, premium and expressive. No skeuomorphic
  or playful styling.
- **Delivery:** static build embedded into `koine-server` via rust-embed. Single binary:
  `./koine serve` gives broker + UI. Live updates over SSE/WebSocket from the control plane.
- Rejected alternatives: Leptos (painful d3 interop, slow visual iteration, immature
  component ecosystem for the design bar); copying the todo-app Next.js frontend (26k LOC of
  todo logic + Apollo/GraphQL we don't expose + Node runtime breaking single-binary).

---

## 6. Build phases

| Phase | Deliverable | Success criterion |
|---|---|---|
| **0 — Foundations** | Workspace, `.apptlas/`, AGENTS.md, ADRs 0001–0009, CI, lefthook | `cargo build` + full CI green from first push |
| **1 — Event-sourced core** | `Job` domain + ports + postgres/memory adapters + outbox + dispatch projection; enqueue→lease→ack/fail→retry→park via use cases | All three test rings pass; projection replay works |
| **2 — Data plane** | `koine-proto` v1, gRPC server, leases/heartbeats end-to-end, minimal Python SDK, conformance suite | A real Python worker processes jobs with demonstrable crash recovery |
| **3 — Control plane + dashboard v0** | REST+OpenAPI, CLI (`koine trace`), full observability; dashboard: live queues, job history, event timeline | OTel traces visible producer→worker cross-language; dashboard live over SSE |
| **4 — Agentic** | MCP adapter, rich history projections, dashboard v1 (d3 causal trace graph) | An agent enqueues, inspects history, and operates via MCP |
| **5 — Durable execution** | Checkpoints, signals, approvals, **repair & resume** (API + dashboard UX) | An agentic job fails at step N, is repaired, resumes from checkpoint |

**Vacation-week target:** phases 0–3 at cathedral quality, stretch goal 4. Dashboard evolves
in parallel from phase 3 onward (earlier scaffolding is fine once REST read endpoints exist).

### Out of scope for this spec (future specs)

DAG workflows/batches as user-facing features, additional SDKs (Go/Node/Java), clustering/HA,
web dashboard advanced analytics, cron scheduling UI, migration tools from Faktory/Sidekiq.
The architecture reserves room for all of these (saga orchestrator, event schema, protocol
versioning) without committing to their design now.

---

## 7. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Event-sourcing hot-path throughput on Postgres | Synchronous dispatch projection + SKIP LOCKED is a known-fast pattern; benchmark in phase 2; event store behind a port allows future embedded backend |
| Scope creep (the 2025 draft failure mode) | Phased plan with per-phase success criteria; durable execution is schema-reserved but not built until phase 5 |
| Polyglot promise degrades into "works in Python only" | Conformance suite is a phase-2 deliverable, before any second SDK |
| Solo-maintainer bus factor for an ambitious OSS project | Cathedral-grade governance (.apptlas, ADRs, AGENTS.md) makes the repo agent-operable and contributor-friendly from day one |
| `koine` name/crate squatting before publish | Reserve crates.io names and GitHub repo early in phase 0 |
