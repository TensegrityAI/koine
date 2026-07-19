# Koiné Phase 2A — Data-Plane Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Koiné its wire: the TLA+ lease-protocol model checked by TLC (in CI), the versioned `koine.v1` proto contract, and a production gRPC data-plane server — authenticated, LISTEN/NOTIFY-woken, worker-presence-aware — proven over real sockets against real Postgres.

**Architecture:** Model first (epic item 1): the TLA+ skeleton from 1A grows lease identity and late-ack actions, TLC checks the safety properties, and only then does the wire land. `koine-proto` owns the versioned contract (ADR 0013: server-streaming Fetch + unary control ops, JSON payloads, additive-only evolution). `koine-grpc` is a thin driving adapter: every RPC maps to an existing use case; auth is a bearer-token interceptor (ADR 0014); fetch wakeups ride a new `DispatchSignal` port (Postgres LISTEN/NOTIFY; tokio Notify in memory). Worker presence is ephemeral state (ADR 0015) — a table, not events. Phase 2B (separate plan, written after 2A executes) adds the Python SDK, ring-4 conformance suite, benchmarks, and crates.io publication.

**Tech Stack:** TLA+/TLC (tla2tools.jar, Java on runners), tonic 0.14 + prost (pinned workspace-consistent), tokio, sqlx PgListener.

**Reference:** epic `.apptlas/epics/phase-2-data-plane.md` items 1–6 (items 7–12 → 2B); auth scope ratified 2026-07-18; spec §2/§3; ADRs 0004–0012; carryover item `.apptlas/backlog/todo/phase-2-carryover-hardening.md` (folded in where marked). Phase 1 delivered: domain (19-kind taxonomy), ports, memory + Postgres stores (75 tests), dev-loop.

## Global Constraints

- Everything from phases 0–1 still binds: strict lints (clippy pedantic, `-D warnings`, `missing_docs`), no `unwrap`/`expect` outside tests, integration-test files open with `#![allow(clippy::expect_used)]` + rationale, TDD, Conventional Commits ≤72, `make ci` green per commit, event log append-only, kind strings and **proto field numbers** immutable once committed.
- Style: plain `async fn` in trait impls where possible; no `manual_async_fn` allows; disclose ALL deviations honestly (two phase-1 reports were corrected for false "No Deviations" claims — the reviewer will diff your claim against the file list).
- Dependency direction: `koine-proto` standalone; `koine-grpc` → proto + application + domain (NEW edge to proto — recorded in ADR 0013); `koine-server` gains proto/grpc wiring. No other new edges.
- Proto contract: package `koine.v1`; **additive-only within v1** — new fields optional with new numbers, removed fields `reserved`; breaking = new package. JSON payloads as `string` fields (mirrors JSONB truth, ADR 0010).
- TLC must pass in CI (new `tla` job) and via `make tla`; model changes and state-machine changes ship together (drift discipline applies to models — phase-2 epic risk).
- Auth: every data-plane RPC requires `authorization: Bearer $KOINE_WORKER_TOKEN` + `koine-worker-id` metadata (validated `WorkerId`); missing/wrong → `UNAUTHENTICATED`. TLS is proxy-terminated in v1 (ADR 0014 guidance); never claim native TLS exists.
- Migrations append-only: worker presence is `migrations/0002_worker_presence.sql`, never edits 0001.

## File map

| File | Responsibility |
| --- | --- |
| `docs/adr/{0013,0014,0015}-*.md` + INDEX rows | Wire contract; worker auth v1; worker presence as ephemeral state |
| `docs/formal/lease_protocol.tla` (rewrite) + `lease_protocol.cfg` + `README.md` | Checked model: lease identity, late acks, attempt cap |
| `Makefile` (`tla` target), `.github/workflows/ci.yml` (`tla` job) | TLC gates |
| `crates/koine-proto/proto/koine/v1/worker.proto` | The v1 contract |
| `crates/koine-proto/{build.rs,src/lib.rs,Cargo.toml}` | tonic/prost codegen |
| `crates/koine-application/src/ports.rs` | + `DispatchSignal` port |
| `crates/koine-store-memory/src/signal.rs` | `NotifySignal` (tokio) |
| `crates/koine-store-postgres/src/signal.rs` + NOTIFY in `store.rs` | `PgSignal` (LISTEN/NOTIFY) |
| `crates/koine-store-postgres/migrations/0002_worker_presence.sql` + `src/presence.rs` | `workers` table + upsert/list |
| `crates/koine-grpc/src/{lib,auth,service}.rs` | WorkerService impl + bearer interceptor |
| `crates/koine-grpc/tests/wire.rs` | In-process duplex-channel tests over memory store |
| `crates/koine-server/src/{main,serve}.rs` | `koine-server serve` (bind, graceful shutdown) |
| `crates/koine-store-postgres/tests/grpc_e2e.rs` | Ring-3: real socket + real Postgres crash story |
| `docs/architecture/*` + records | Closeout 2A |

---

### Task 1: ADRs 0013–0015

**Files:**
- Create: `docs/adr/0013-wire-contract-v1.md`, `docs/adr/0014-worker-auth-v1.md`, `docs/adr/0015-worker-presence-ephemeral.md`; add 3 rows to `docs/adr/INDEX.md`

**Interfaces:**
- Consumes: spec §2/§3; ratified auth decision (epic item 5); WorkerRegistration disposition (phase-1 epic item 1).
- Produces: the decisions Tasks 3–8 implement.

- [ ] **Step 1: Write ADR 0013**

```markdown
# 0013 — Wire contract v1

- **Status:** accepted
- **Date:** 2026-07-19
- **Context:** The data plane needs its versioned contract (spec §2: "the
  proto IS the product's polyglot promise"). Decisions needed: RPC shape,
  payload encoding, evolution policy, and the new compile-time edge
  `koine-grpc → koine-proto`.
- **Decision:**
  - Package **`koine.v1`**, one file `koine/v1/worker.proto` (data plane
    only; the control plane is REST in phase 3).
  - **RPC shape: server-streaming `Fetch` + unary control ops**
    (`Start`, `Succeed`, `Fail`, `Heartbeat`). The server streams
    `LeasedJob`s as they become claimable (long-lived stream, wakeup via
    `DispatchSignal`); acks are unary because they are individually
    meaningful, retryable, and map 1:1 to use cases. The spec §2 diagram
    says "bidi-stream"; a full bidi protocol multiplexing acks into the
    stream adds session-state complexity v1 does not need — recorded as a
    documented divergence; re-evaluate with phase-2B benchmarks.
  - **Payloads are JSON strings** (`payload_json`, `result_json`,
    structured `JobError` fields): mirrors the JSONB source of truth
    (ADR 0010), keeps every SDK trivial (parse JSON, no nested proto
    schema per job type). Timestamps are `int64` unix milliseconds
    (`expires_at_unix_ms`) — no well-known-type dependency in SDKs.
  - **Evolution: additive-only within v1.** New fields = new numbers,
    optional semantics; removed fields become `reserved N;` forever; field
    numbers and names are wire contract like event kinds. Breaking changes
    = `koine.v2` package side-by-side. The ring-4 conformance suite (2B)
    is the compatibility gate.
  - New dependency edge `koine-grpc → koine-proto` (and `koine-server →`
    both) — recorded here per AGENTS.md's edge rule.
- **Consequences:** SDKs are generated + thin; JSON payload cost accepted
  until benchmarks argue otherwise; the stream's lease TTL is chosen by the
  worker per Fetch (bounded server-side); divergence from the spec diagram
  is on record with its revisit trigger.
- **Alternatives considered:** full bidi stream (session-state complexity,
  ack ordering ambiguity); unary long-poll `LeaseNext` (simplest, but
  per-claim RTT and no server push); protobuf-native payloads (couples every
  job type to proto schema churn); google.protobuf.Timestamp/Struct (drags
  well-known types into every SDK).
```

- [ ] **Step 2: Write ADR 0014**

```markdown
# 0014 — Worker auth v1

- **Status:** accepted
- **Date:** 2026-07-19
- **Context:** Ratified scope (maintainer decision 2026-07-18): the data
  plane must not ship unauthenticated, and auth added after the v1 contract
  freeze would be breaking. v1 needs a minimal credible scheme, not an
  identity platform.
- **Decision:**
  - **Single shared bearer token per deployment**: server reads
    `KOINE_WORKER_TOKEN`; every RPC (including the Fetch stream's initial
    call) must carry metadata `authorization: Bearer <token>` — enforced by
    a tonic interceptor; failures return `UNAUTHENTICATED` with no detail
    leakage.
  - **Worker identity is claimed, not proven, in v1**: metadata
    `koine-worker-id`, validated as a domain `WorkerId` (non-empty, ≤256
    bytes, no control chars) — it scopes leases and presence, not
    privileges. All authenticated workers are equal.
  - **Transport security is proxy-terminated in v1**: deploy behind
    TLS-terminating ingress (documented guidance); the server binds plain
    HTTP/2. Native rustls and mTLS are explicitly NOT claimed.
  - Constant-time token comparison (subtle crate or length-guarded ct_eq)
    to avoid trivial timing oracles.
- **Consequences:** one secret to rotate (rotation = restart, acceptable
  v1); a leaked token grants full worker capability — documented; per-worker
  tokens/mTLS/OIDC become a phase-4+ backlog item with real requirements.
- **Alternatives considered:** no auth (fails the ratified scope); per-worker
  static tokens (secret sprawl without a management plane); mTLS (operational
  cost before any multi-tenant need); OIDC (an identity platform, not v1).
```

- [ ] **Step 3: Write ADR 0015**

```markdown
# 0015 — Worker presence as ephemeral state

- **Status:** accepted
- **Date:** 2026-07-19
- **Context:** Spec §2 lists `WorkerRegistration` among domain aggregates;
  the phase-1 disposition deferred it to phase 2 "where workers first
  connect." But spec §3's event taxonomy defines zero worker events, and
  the spec's own doctrine makes high-frequency liveness data ephemeral
  (heartbeats). The two spec signals conflict; this ADR resolves it.
- **Decision:** worker presence is **ephemeral infrastructure state**, like
  lease deadlines (ADR 0011-c): a `workers` table (`worker_id` PK,
  `first_seen`, `last_seen`, `last_queue`) upserted on every authenticated
  data-plane call. No domain events, no aggregate, no stream. It feeds
  phase-3 dashboards and operational queries (`SELECT … WHERE last_seen >
  now() - interval '1 minute'`).
- **Consequences:** no audit history of worker fleet churn (revisit if a
  real consumer appears — that would justify event-sourcing worker
  lifecycle and generifying `EventStore`, the recorded 1A trigger);
  presence survives restarts as stale rows — readers filter by `last_seen`;
  the spec §2 aggregate list is superseded on this point by this ADR
  (spec-fidelity: divergence with disposition, recorded here).
- **Alternatives considered:** event-sourced WorkerRegistration aggregate
  (second aggregate would force EventStore generification now, for data
  nobody consumes yet); in-memory-only presence (lost on restart, invisible
  to operators querying the DB); no presence at all (phase-3 dashboards
  would have nothing to show for the fleet).
```

- [ ] **Step 4: INDEX rows + gate + commit**

Append to `docs/adr/INDEX.md`:

```markdown
| [0013](0013-wire-contract-v1.md) | Wire contract v1 | accepted | 2026-07-19 |
| [0014](0014-worker-auth-v1.md) | Worker auth v1 | accepted | 2026-07-19 |
| [0015](0015-worker-presence-ephemeral.md) | Worker presence as ephemeral state | accepted | 2026-07-19 |
```

Run: `make md && typos`
Expected: clean.

```bash
git add docs/adr
git commit -m "docs: accept ADRs 0013-0015 for the data plane"
```

---

### Task 2: TLA+ model completion + TLC gate

**Files:**
- Rewrite: `docs/formal/lease_protocol.tla`; Create: `docs/formal/lease_protocol.cfg`; Modify: `docs/formal/README.md`, `Makefile` (+`tla` target, add to `ci` chain? NO — separate target + CI job, TLC needs Java), `.github/workflows/ci.yml` (+`tla` job)

**Interfaces:**
- Consumes: the 1A skeleton; `koine-domain/src/job.rs` transition table (the model must mirror it).
- Produces: a TLC-checked model; the `make tla` gate Task 10's closeout cites.

- [ ] **Step 1: Rewrite the model with lease identity and late acks**

`docs/formal/lease_protocol.tla`:

```text
---- MODULE lease_protocol ----
(* Phase 2A: checked model of Koiné's lease/delivery protocol for ONE job.
   Mirrors koine-domain's Job state machine (job.rs transition table).
   Scope: lease identity, expiry, late acks, attempt cap. Atomicity note:
   each action models one database transaction (ADR 0011/0012) — the
   SKIP LOCKED claim is atomic BY CONSTRUCTION here; the implementation's
   obligation is exactly that atomicity. Multi-job/queue ordering is out of
   scope (covered by ring-3/ring-4 tests). *)

EXTENDS Naturals, FiniteSets

CONSTANTS Workers, MaxAttempts, MaxLeases

VARIABLES state, attempt, activeLease, issued, conflicts

vars == <<state, attempt, activeLease, issued, conflicts>>

States == {"pending", "leased", "running", "succeeded", "parked", "cancelled"}
Terminal == {"succeeded", "cancelled"}
NoLease == 0

Init ==
    /\ state = "pending"
    /\ attempt = 0
    /\ activeLease = NoLease
    /\ issued = 0
    /\ conflicts = 0

(* A worker claims the job: one atomic tx issues a fresh lease id. *)
Lease ==
    /\ state = "pending"
    /\ issued < MaxLeases
    /\ issued' = issued + 1
    /\ activeLease' = issued + 1
    /\ state' = "leased"
    /\ UNCHANGED <<attempt, conflicts>>

Start ==
    /\ state = "leased"
    /\ state' = "running"
    /\ UNCHANGED <<attempt, activeLease, issued, conflicts>>

(* Ack with the CURRENT lease: normal completion. *)
AckSucceed(l) ==
    /\ state = "running" /\ l = activeLease
    /\ state' = "succeeded"
    /\ activeLease' = NoLease
    /\ UNCHANGED <<attempt, issued, conflicts>>

AckFail(l) ==
    /\ state = "running" /\ l = activeLease
    /\ attempt' = attempt + 1
    /\ activeLease' = NoLease
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
    /\ UNCHANGED <<issued, conflicts>>

(* Sweep: the lease deadline passed. *)
Expire ==
    /\ state \in {"leased", "running"}
    /\ attempt' = attempt + 1
    /\ activeLease' = NoLease
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
    /\ UNCHANGED <<issued, conflicts>>

(* A STALE ack (lease no longer active): recorded as a conflict event,
   lifecycle state untouched — spec §3 "information is never lost". *)
LateAck(l) ==
    /\ l # activeLease /\ l >= 1 /\ l <= issued
    /\ conflicts' = conflicts + 1
    /\ UNCHANGED <<state, attempt, activeLease, issued>>

Cancel ==
    /\ state \in {"pending", "leased", "running", "parked"}
    /\ state' = "cancelled"
    /\ activeLease' = NoLease
    /\ UNCHANGED <<attempt, issued, conflicts>>

Next ==
    \/ Lease \/ Start \/ Expire \/ Cancel
    \/ \E l \in 1..MaxLeases : AckSucceed(l) \/ AckFail(l) \/ LateAck(l)

Spec == Init /\ [][Next]_vars /\ WF_vars(Lease) /\ WF_vars(Expire)

----
(* PROPERTIES *)

TypeOK ==
    /\ state \in States
    /\ attempt \in 0..MaxAttempts
    /\ activeLease \in 0..MaxLeases
    /\ issued \in 0..MaxLeases
    /\ conflicts \in Nat

(* At most one live lease ever exists — by construction each Lease retires
   the notion of eligibility until Expire/AckFail return the job to pending,
   and activeLease is a single register. *)
NoDualLease == (state \in {"leased", "running"}) => activeLease # NoLease

(* A lease id is never reused. *)
FreshLeases == activeLease <= issued

(* Late acks never corrupt lifecycle state: proven structurally by LateAck's
   UNCHANGED clause; TypeOK + the state machine make it checkable. *)
AttemptCapped == attempt <= MaxAttempts

(* Liveness (under fairness of Lease and Expire): the job always reaches a
   terminal state or parks — no livelock where it pends forever. *)
EventuallySettled == <>[](state \in Terminal \cup {"parked"})
====
```

- [ ] **Step 2: Write the TLC config**

`docs/formal/lease_protocol.cfg`:

```text
SPECIFICATION Spec
CONSTANTS
    Workers = {w1, w2}
    MaxAttempts = 3
    MaxLeases = 5
INVARIANTS
    TypeOK
    NoDualLease
    FreshLeases
    AttemptCapped
PROPERTIES
    EventuallySettled
```

(Note: `Workers` is declared for future multi-worker refinement; the current
actions don't index on it — TLC accepts unused constants.)

- [ ] **Step 3: `make tla` target + tooling**

Append to `Makefile` (TAB recipes; add `tla` to `.PHONY`):

```makefile
TLA_TOOLS := docs/formal/.tools/tla2tools.jar

$(TLA_TOOLS):
	mkdir -p docs/formal/.tools
	curl -fsSL https://github.com/tlaplus/tlaplus/releases/latest/download/tla2tools.jar -o $(TLA_TOOLS)

tla: $(TLA_TOOLS)
	cd docs/formal && java -XX:+UseParallelGC -jar .tools/tla2tools.jar -config lease_protocol.cfg lease_protocol.tla
```

Add `docs/formal/.tools/` to `.gitignore`. Verify Java: `java -version` (present on dev box and ubuntu runners; if absent locally, install a JRE and record it).

Run: `make tla`
Expected: TLC output ending `Model checking completed. No error has been found.` — if TLC reports a counterexample, that is a REAL protocol/model bug: analyze the trace; if the MODEL diverges from `job.rs`, fix the model; if the trace shows a genuine protocol flaw, STOP and escalate (phase-2 epic risk: back-propagates as a phase-1 fidelity finding).

- [ ] **Step 4: CI job**

Add to `.github/workflows/ci.yml` after the `markdownlint` job:

```yaml
  tla:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-java@v4
        with:
          distribution: temurin
          java-version: "21"
      - run: make tla
```

- [ ] **Step 5: Update `docs/formal/README.md`** — replace the "skeleton" status: model is now TLC-checked (list the four invariants + liveness property, the `make tla` command, the CI job, the single-job scope note, and the drift rule: `job.rs` transition-table changes and model changes ship in the same PR).

- [ ] **Step 6: Gate + commit**

Run: `make tla && make ci`

```bash
git add docs/formal Makefile .gitignore .github/workflows/ci.yml
git commit -m "feat(formal): check lease protocol model with tlc in ci"
```

---

### Task 3: `koine-proto` — the v1 contract + codegen

**Files:**
- Create: `crates/koine-proto/proto/koine/v1/worker.proto`, `crates/koine-proto/build.rs`
- Modify: `crates/koine-proto/Cargo.toml`, `crates/koine-proto/src/lib.rs`

**Interfaces:**
- Consumes: ADR 0013.
- Produces: generated `koine_proto::v1::{worker_service_server, worker_service_client, FetchRequest, LeasedJob, StartRequest, StartResponse, SucceedRequest, FailRequest, JobError, AckOutcome, AckResponse, HeartbeatRequest, HeartbeatResponse}` — Tasks 5/6/9 and phase 2B's SDKs consume these names.

- [ ] **Step 1: Write the proto**

`crates/koine-proto/proto/koine/v1/worker.proto`:

```proto
syntax = "proto3";

package koine.v1;

// Koiné data plane v1 (ADR 0013). Field numbers are wire contract:
// additive-only within v1; removed fields become `reserved`.

service WorkerService {
  // Long-lived stream of claimed jobs for one worker on one queue.
  // Worker identity and auth ride the request metadata (ADR 0014).
  rpc Fetch(FetchRequest) returns (stream LeasedJob);
  rpc Start(StartRequest) returns (StartResponse);
  rpc Succeed(SucceedRequest) returns (AckResponse);
  rpc Fail(FailRequest) returns (AckResponse);
  rpc Heartbeat(HeartbeatRequest) returns (HeartbeatResponse);
}

message FetchRequest {
  string queue = 1;
  // Lease TTL granted per claimed job; server clamps to its ceiling.
  uint64 lease_ttl_ms = 2;
}

message LeasedJob {
  string job_id = 1;          // UUID
  string queue = 2;
  string payload_json = 3;    // opaque JSON (ADR 0013)
  uint32 attempt = 4;         // completed attempts before this lease
  string lease_id = 5;        // UUID — ack with exactly this
  int64 expires_at_unix_ms = 6;
  string correlation_id = 7;  // UUID
  optional string traceparent = 8;
}

message StartRequest {
  string job_id = 1;
}
message StartResponse {}

message SucceedRequest {
  string job_id = 1;
  string lease_id = 2;
  optional string result_json = 3;
}

message FailRequest {
  string job_id = 1;
  string lease_id = 2;
  JobError error = 3;
}

message JobError {
  string kind = 1;
  string message = 2;
  optional string stacktrace = 3;
  bool retryable = 4;
}

enum AckOutcome {
  ACK_OUTCOME_UNSPECIFIED = 0;
  ACK_OUTCOME_RECORDED = 1;
  // Lease no longer held: recorded as late_ack_conflict, never lost.
  ACK_OUTCOME_CONFLICT = 2;
}

message AckResponse {
  AckOutcome outcome = 1;
}

message HeartbeatRequest {
  string lease_id = 1;
  uint64 ttl_ms = 2;
}
message HeartbeatResponse {
  // false = lease gone; the worker must stop working this job.
  bool alive = 1;
}
```

- [ ] **Step 2: Codegen wiring**

`crates/koine-proto/Cargo.toml` deps:

```toml
[dependencies]
prost = "0.14"
tonic = { version = "0.14", default-features = false, features = ["codegen", "transport"] }
tonic-prost = "0.14"

[build-dependencies]
tonic-prost-build = "0.14"
```

(tonic 0.14 split codegen into the `tonic-prost*` crates; if the resolver disagrees with these exact names/features, use the pairing tonic 0.14's own docs specify, record exact versions as a deviation, and keep `koine-proto` free of any other deps.)

`crates/koine-proto/build.rs`:

```rust
//! Generates the koine.v1 gRPC contract at build time.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/koine/v1/worker.proto"], &["proto"])?;
    Ok(())
}
```

`crates/koine-proto/src/lib.rs` (replace stub body, keep first doc line):

```rust
//! Koiné wire contract: versioned protobuf definitions and generated gRPC types.

/// The koine.v1 data-plane contract (generated; see ADR 0013).
#[allow(missing_docs, clippy::pedantic, clippy::nursery)] // generated code
pub mod v1 {
    tonic::include_proto!("koine.v1");
}
```

- [ ] **Step 3: Gate + commit**

Run: `cargo build -p koine-proto && cargo clippy -p koine-proto --all-targets -- -D warnings && make ci`
Expected: green (generated code exempted by the module-level allows; if clippy still fires through, widen the allow list minimally with a comment, disclose).

```bash
git add crates/koine-proto Cargo.lock
git commit -m "feat(proto): add koine.v1 worker contract with tonic codegen"
```

---

### Task 4: `DispatchSignal` + `WorkerPresence` ports and adapters

**Files:**
- Modify: `crates/koine-application/src/ports.rs` (+2 ports), `crates/koine-application/src/lib.rs` (re-exports)
- Create: `crates/koine-store-memory/src/signal.rs` (+ no-op presence), `crates/koine-store-postgres/src/signal.rs`, `crates/koine-store-postgres/migrations/0002_worker_presence.sql`, `crates/koine-store-postgres/src/presence.rs`
- Modify: `crates/koine-store-postgres/src/store.rs` (NOTIFY on Pending projection), both stores' `lib.rs`

**Interfaces:**
- Consumes: existing ports/types.
- Produces:

```rust
/// Wakeup channel for dispatch availability (fetch streams, ADR 0013).
pub trait DispatchSignal: Send + Sync {
    /// Announces that `queue` may have claimable work.
    fn notify(&self, queue: &QueueName) -> impl Future<Output = ()> + Send;
    /// Waits until `queue` is signaled or `timeout` elapses. Spurious
    /// wakeups are allowed; callers re-check by claiming.
    fn wait(&self, queue: &QueueName, timeout: Duration) -> impl Future<Output = ()> + Send;
}

/// Ephemeral worker presence (ADR 0015).
pub trait WorkerPresence: Send + Sync {
    /// Records that `worker` was seen now (optionally on `queue`).
    fn seen(
        &self,
        worker: &WorkerId,
        queue: Option<&QueueName>,
    ) -> impl Future<Output = ()> + Send;
}
```

plus `NotifySignal::new()` + `NoopPresence` (memory crate) and `PgSignal::new(pool)` + `PgPresence::new(pool)` (postgres crate). Postgres `append` NOTIFYs channel `koine_dispatch` with the queue name whenever projection yields a Pending row (in-tx `pg_notify` → delivered on commit).

- [ ] **Step 1: Failing tests** — write these concretely:
- `crates/koine-store-memory/src/signal.rs` `#[cfg(test)]`: (1) `wait` returns promptly after a concurrent `notify` on the same queue (spawn a task that notifies after 50ms; wait with 5s timeout; assert elapsed < 1s); (2) `wait` on a DIFFERENT queue times out at ~`timeout` (100ms timeout; assert elapsed >= 100ms); (3) `NoopPresence::seen` completes (smoke).
- `crates/koine-store-postgres/tests/signal.rs` (ring 3, standard header + `mod support;`): (1) `PgSignal::wait` wakes when an ELIGIBLE job is appended to that queue via `PostgresEventStore::append` from another task (assert elapsed well under the 5s timeout); (2) wait on another queue times out at ~300ms; (3) presence: `PgPresence::seen(worker, Some(queue))` twice → one `workers` row, `last_seen` advanced, `last_queue` set (query the table directly).

- [ ] **Step 2: Implement.** Memory `NotifySignal`: `Mutex<HashMap<QueueName, Arc<tokio::sync::Notify>>>`; `notify` → `notify_waiters()` on the entry (create if absent); `wait` → clone the Arc, `tokio::time::timeout(timeout, notified()).await` ignoring the result. (`tokio` moves from dev-dependency to real dependency of `koine-store-memory` with features `["sync", "time", "macros", "rt"]` — disclose in the report; it remains a test-support crate.) `NoopPresence`: empty async body.
Postgres `PgSignal`: `notify` → `SELECT pg_notify('koine_dispatch', $1)` bound to `queue.as_str()`; `wait` → `sqlx::postgres::PgListener::connect_with(&pool)` (per call — v1 simplicity, perf note for 2B bench), `listen("koine_dispatch")`, then loop on `tokio::time::timeout(remaining, listener.recv())`: payload == queue → return; other payload → continue with remaining budget; timeout/error → return. `PgPresence::seen`: `INSERT INTO event_store.workers (worker_id, first_seen, last_seen, last_queue) VALUES ($1, now(), now(), $2) ON CONFLICT (worker_id) DO UPDATE SET last_seen = now(), last_queue = COALESCE(EXCLUDED.last_queue, event_store.workers.last_queue)`; errors are logged-and-swallowed? NO — presence must not fail requests: return `()` but `debug_assert!`-free; swallow the error silently is fake-completeness-adjacent — decision: the port returns `()`; the pg impl ignores DB errors by design (presence is best-effort, ADR 0015) with an inline comment stating exactly that.
Migration `0002_worker_presence.sql`:

```sql
CREATE TABLE event_store.workers (
    worker_id  TEXT PRIMARY KEY,
    first_seen TIMESTAMPTZ NOT NULL,
    last_seen  TIMESTAMPTZ NOT NULL,
    last_queue TEXT
);
```

Store NOTIFY: in `project_in_tx`'s Pending branch, after the upsert: `sqlx::query("SELECT pg_notify('koine_dispatch', $1)").bind(job.queue.as_str()).execute(&mut **tx)`.

- [ ] **Step 3: Gate + commit**

Run: `cargo test -p koine-store-memory && cargo test -p koine-store-postgres && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`

```bash
git add crates/koine-application crates/koine-store-memory crates/koine-store-postgres
git commit -m "feat(application): add dispatch-signal and presence ports"
```

---

### Task 5: `koine-grpc` — auth interceptor + WorkerService over use cases

**Files:**
- Create: `crates/koine-grpc/src/auth.rs`, `crates/koine-grpc/src/service.rs`
- Modify: `crates/koine-grpc/src/lib.rs`, `crates/koine-grpc/Cargo.toml`

**Interfaces:**
- Consumes: `koine_proto::v1::*`, use cases (`LeaseNextJob`, `WorkerAck`, `Heartbeat`), ports (`EventStore`, `Dispatcher`, `IdGenerator`, `Clock`, `DispatchSignal`, `WorkerPresence`), domain ids.
- Produces: `WorkerApi<S, D, G, C, Sig, P>` implementing the generated `worker_service_server::WorkerService`; `WorkerApi::new(deps: Arc<Deps<…>>)` where `Deps { store, dispatcher, ids, clock, signal, presence, config: GrpcConfig { token: String, max_lease_ttl: Duration, idle_poll: Duration } }`; `auth::check(metadata, token) -> Result<WorkerId, tonic::Status>`; `pub fn server(deps) -> WorkerServiceServer<WorkerApi<…>>`. Tasks 6/8/9 consume these.

- [ ] **Step 1: `Cargo.toml`** — add: `koine-proto` path dep (+version "0.1.0"), `tonic = { version = "0.14", default-features = false, features = ["transport", "codegen"] }`, `prost = "0.14"`, `tokio = { version = "1", features = ["sync", "time", "rt", "macros"] }`, `tokio-stream = "0.1"`, `serde_json = "1"`, `subtle = "2"`, `chrono = "0.4"`, `uuid = { version = "1", features = ["v7"] }`, `thiserror = "2"`. Dev-deps: `koine-store-memory` path, `tower = "0.5"`, `hyper-util = "0.1"` (duplex connector — adjust to what tonic 0.14's testing pattern needs; record exact).

- [ ] **Step 2: `auth.rs`** — write concretely:

```rust
//! Bearer-token + worker-identity extraction (ADR 0014).

use koine_domain::WorkerId;
use subtle::ConstantTimeEq as _;
use tonic::metadata::MetadataMap;
use tonic::Status;

/// Validates `authorization: Bearer <token>` (constant-time) and the
/// `koine-worker-id` header. Returns the caller's `WorkerId`.
///
/// # Errors
/// `UNAUTHENTICATED` on any missing/invalid credential — no detail leakage.
pub fn check(metadata: &MetadataMap, expected_token: &str) -> Result<WorkerId, Status> {
    let unauthenticated = || Status::unauthenticated("invalid credentials");
    let auth = metadata
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthenticated)?;
    let presented = auth.strip_prefix("Bearer ").ok_or_else(unauthenticated)?;
    let token_ok = presented.len() == expected_token.len()
        && bool::from(presented.as_bytes().ct_eq(expected_token.as_bytes()));
    if !token_ok {
        return Err(unauthenticated());
    }
    let worker = metadata
        .get("koine-worker-id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(unauthenticated)?;
    WorkerId::new(worker).map_err(|_| unauthenticated())
}
```

Inline `#[cfg(test)]`: valid pair → Ok(worker); wrong token (same length AND different length) → err; missing header → err; bad worker id (empty/control chars) → err; token comparison of equal-length wrong token exercises the ct_eq path.

- [ ] **Step 3: `service.rs`** — STRUCTURAL SPEC (implementer writes every line; the mappings are exact):
- `Deps<…>` + `GrpcConfig` structs as in Interfaces; `WorkerApi` holds `Arc<Deps<…>>`.
- `impl WorkerService for WorkerApi`:
  - `type FetchStream = ReceiverStream<Result<v1::LeasedJob, Status>>` (tokio_stream wrapper over `mpsc::channel(16)`).
  - `fetch`: `auth::check` → parse+validate `QueueName::new(req.queue)` (`InvalidArgument` on error) → clamp `lease_ttl_ms` to `config.max_lease_ttl` (0 → `InvalidArgument`) → `presence.seen(&worker, Some(&queue))` → spawn a task looping: `LeaseNextJob { dispatcher }.execute(&queue, &worker, ttl)`: `Ok(Some(job))` → map to proto (`job_id/lease_id/correlation_id` via `.to_string()`, `payload_json` via `serde_json::to_string` — encode error → `Status::internal("payload encode")` sent then break; `expires_at_unix_ms` via `timestamp_millis()`) and `tx.send(Ok(...))` (receiver gone → break); `Ok(None)` → `signal.wait(&queue, config.idle_poll)` then continue; `Err(e)` → send `Status::unavailable(format!("dispatch: {e}"))` and break. (The abandoned stream's claimed-but-unsent job is covered by lease expiry + sweep — note this in a comment: crash-safety by design, ADR 0008.)
  - `start`: auth → parse `job_id` UUID (`InvalidArgument`) → `presence.seen` → `WorkerAck { store, ids, clock }.start(job_id, &worker)`: Ok → `StartResponse {}`; `AckError::Domain(_)` → `Status::failed_precondition` (worker must refetch); `AckError::Store(EventStoreError::StreamNotFound(_))` → `Status::not_found`; other store errors → `Status::internal`.
  - `succeed`/`fail`: auth → parse `job_id`+`lease_id` UUIDs → optional `result_json` parsed via `serde_json::from_str` (`InvalidArgument` on malformed) / `FailRequest.error` mapped to domain `JobError` (missing `error` field → `InvalidArgument`) → `WorkerAck::succeed/fail` → map `AckOutcome::{Recorded→ACK_OUTCOME_RECORDED, Conflict→ACK_OUTCOME_CONFLICT}`; error mapping as `start`.
  - `heartbeat`: auth → parse lease UUID → `Heartbeat { dispatcher }.execute(lease, ttl)` → `HeartbeatResponse { alive }`; dispatch errors → `Status::internal`.
- `pub fn server(deps: Arc<Deps<…>>) -> WorkerServiceServer<WorkerApi<…>>` constructor.
- `lib.rs`: `pub mod auth; pub mod service;` + re-exports; drop stub lines.

- [ ] **Step 4: Gate + commit**

Run: `cargo build -p koine-grpc && cargo clippy -p koine-grpc --all-targets -- -D warnings && cargo test -p koine-grpc && cargo fmt --all`

```bash
git add crates/koine-grpc Cargo.lock
git commit -m "feat(grpc): add authenticated worker service over use cases"
```

---

### Task 6: Wire tests — in-process server over the memory store

**Files:**
- Create: `crates/koine-grpc/tests/wire.rs`

**Interfaces:** consumes Task 5's `server(deps)` + memory adapters + generated client.

- [ ] **Step 1: Harness** — standard header; helper `spawn_server() -> (WorkerServiceClient<Channel>, TestWorld)` using the tonic in-process pattern: bind `tonic::transport::Server` on a `tokio::io::duplex` pair via `Endpoint::connect_with_connector` + `tower::service_fn` (or, if the 0.14 duplex pattern fights, bind on `127.0.0.1:0` TCP and connect to the assigned port — either is acceptable; record which). `TestWorld` mirrors the ring-2 `World` (memory store/dispatcher/`NotifySignal`/`NoopPresence`/FixedClock/SeededIds, token "test-token"). Client helper `authed(req)` attaches `authorization` + `koine-worker-id` metadata.

- [ ] **Step 2: The suite** — write these six tests concretely:
1. `unauthenticated_calls_are_rejected` — no metadata → `Code::Unauthenticated` on `start`; wrong same-length token too.
2. `fetch_streams_a_claimed_job` — enqueue via use case, `fetch(queue, ttl 30s)`, first stream item is the job (payload round-trips, attempt 0, UUIDs parse).
3. `full_story_over_the_wire` — fetch → start → succeed(result) → `ACK_OUTCOME_RECORDED`; store stream kinds == `[enqueued, leased, started, succeeded]`.
4. `stale_ack_returns_conflict` — fetch, advance clock past ttl, sweep (construct `SweepExpiredLeases` directly on the world), then `succeed` with the stale lease → `ACK_OUTCOME_CONFLICT`; story contains `late_ack_conflict`.
5. `fetch_wakes_on_late_enqueue` — open fetch on an empty queue; after 100ms enqueue from another task; assert the stream yields within 1s (signal path, not the idle_poll fallback — set `idle_poll` to 10s in this test's config to prove it).
6. `heartbeat_reports_liveness` — fetched job: heartbeat → alive true; advance past ttl; heartbeat → alive false.

- [ ] **Step 3: Gate + commit**

Run: `cargo test -p koine-grpc --test wire && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`

```bash
git add crates/koine-grpc
git commit -m "test(grpc): add in-process wire suite over memory adapters"
```

---

### Task 7: `koine-server serve`

**Files:**
- Create: `crates/koine-server/src/serve.rs`; Modify: `crates/koine-server/src/main.rs`, `crates/koine-server/Cargo.toml` (+koine-proto/koine-grpc path deps if not yet real, `tokio` `signal` feature)

**Interfaces:** consumes everything; produces the `serve` subcommand Task 9's e2e drives via `cargo run`.

- [ ] **Step 1: `serve.rs`** — STRUCTURAL SPEC (write every line): read env `DATABASE_URL` (default as dev-loop), `KOINE_WORKER_TOKEN` (MISSING → return Err("KOINE_WORKER_TOKEN is required") — refuse to start unauthenticated, ADR 0014), `KOINE_GRPC_ADDR` (default `0.0.0.0:7419`), `KOINE_MAX_LEASE_TTL_MS` (default 300_000), `KOINE_IDLE_POLL_MS` (default 1_000). `connect_pool` → build `PostgresEventStore`/`PostgresDispatcher`/`PgSignal`/`PgPresence` + `SystemClock`/`UuidV7Ids` → spawn sweep ticker (500ms: `SweepExpiredLeases::execute`, log count when > 0 via `println!`) + relay ticker (500ms, `PrintingSink` from dev_loop — promote it to a shared `sinks` module or reuse) → `tonic::transport::Server::builder().add_service(koine_grpc::server(deps)).serve_with_shutdown(addr, ctrl_c)`. Print one startup line with addr + queue-agnostic banner. `main.rs`: add `"serve"` arm alongside `"dev-loop"`; update the usage line.

- [ ] **Step 2: Manual smoke (product check, capture in report)** — compose Postgres up (or docker run on 55432), `KOINE_WORKER_TOKEN=t cargo run -p koine-server -- serve` in background, `grpcurl` NOT required: use a 10-line Rust snippet? Simpler: run Task 9's e2e afterwards — for THIS task the smoke is: server starts, prints banner, Ctrl-C shuts down cleanly (send SIGINT, observe exit 0), refuses to start without token (assert error message + non-zero exit). Capture all three observations.

- [ ] **Step 3: Gate + commit**

Run: `make ci`

```bash
git add crates/koine-server Cargo.lock
git commit -m "feat(server): add authenticated grpc serve command"
```

---

### Task 8: Fold-in — carryover hardening item ACs touching 2A

**Files:**
- Modify: `crates/koine-store-postgres/tests/dispatcher.rs` (+Duration::MAX extend test), `crates/koine-store-postgres/tests/replay.rs` (SELECT extended to `not_before, worker_id, lease_expires_at`), `.github/workflows/ci.yml` (+`unused-deps` job), `crates/koine-server/Cargo.toml` (prune now-unused internal deps or wire them — after Task 7, grpc/proto/observability status changes; keep exactly the used set), `.apptlas/backlog/todo/phase-2-carryover-hardening.md` (check the three ACs delivered here; pool-knob + relay/sink note stays for 2B/3)

**Steps (each with run+commit discipline):**
1. Ring-3 test `extend_lease_rejects_unrepresentable_ttl` on `PostgresDispatcher` (mirror the memory twin's assertions; `Duration::MAX`).
2. Replay SELECT columns extended (tuple grows to 7 cols; assertion text unchanged).
3. CI job `unused-deps`: `cargo install cargo-machete --locked` + `cargo machete` (or the maintained equivalent; pin what works, record). Prune `koine-server`'s genuinely-unused deps so the job is green from its first run; `koine-cli` keeps its stub status (machete config `ignored` if needed, with comment).
4. Check the item's ACs with test names; leave remaining ACs unchecked with "→ 2B/3" notes.

Run: `make ci` (now includes unused-deps? NO — machete is a CI job only; add `make machete` target mirroring it for local parity, include in `make ci` chain).
Commit: `test: close 2a-scoped carryover hardening acs`

---

### Task 9: Ring-3 gRPC e2e — the crash story over a real socket

**Files:**
- Create: `crates/koine-grpc/tests/grpc_e2e.rs`; Modify: `crates/koine-grpc/Cargo.toml` (dev-deps: `koine-store-postgres` path, `testcontainers`/`testcontainers-modules` matching workspace versions)

**Steps:**
1. Harness: testcontainers Postgres (copy the `support` pattern — this crate gets its own `tests/support/mod.rs` copy; note the phase-2B dedup follow-up), real `tonic` server on `127.0.0.1:0` (spawned in-process with Postgres deps + real `PgSignal`/`PgPresence`/`SystemClock`/`UuidV7Ids`, token "e2e-token", `idle_poll` 200ms), real client channel to the bound port.
2. THE TEST `crash_recovery_over_the_wire` (write concretely): enqueue via `EnqueueJob` with 2s-ttl fetch → client A fetches the job then DROPS the stream + never acks (simulated crash) → wait ≥ ttl + sweep-tick margin (the server's own sweep ticker isn't running here — drive `SweepExpiredLeases` directly for determinism) → client B fetches: gets the SAME job with `attempt == 1` → B starts + succeeds → `ACK_OUTCOME_RECORDED` → load the story via `PostgresEventStore::load`: kinds contain the full arc `[enqueued, leased, lease_expired, retry_scheduled, leased, started, succeeded]` → A's late `succeed` with its stale lease → `ACK_OUTCOME_CONFLICT`, story gains `late_ack_conflict` at the end.
3. Second test `presence_rows_appear`: after the above, `workers` table has both worker ids with recent `last_seen`.

Run: `cargo test -p koine-grpc --test grpc_e2e && make ci`
Commit: `test(grpc): prove crash recovery over a real socket and store`

---

### Task 10: Closeout 2A

**Files:**
- Create: `docs/architecture/koine-proto.md`, `docs/architecture/koine-grpc.md`; Create `.apptlas/backlog/done/phase-2a-data-plane-server.md`; Modify: `docs/architecture/{README,overview,koine-application,koine-store-memory,koine-store-postgres,koine-server}.md` (new ports/signal/presence/serve — keep every touched page truthful, the 1B lesson), `.apptlas/epics/phase-2-data-plane.md` (State: items 1–6 delivered, 2B next), `CLAUDE.md` (phase log + active plan → 2B pending), `docs/formal/README.md` if not already current

**Steps:**
1. Wiki pages per the four-section template from the delivered code (ADR links: 0013/0014/0015; proto page documents the additive-only policy + field-number immutability; grpc page documents the auth model honestly incl. "TLS is proxy-terminated, not native").
2. Backlog done item (template shape): ACs — TLC green in CI (`tla` job run id); wire suite green; e2e crash story over real socket (test name); auth enforced (test names); wakeup via signal proven (test 5); presence rows (test); carryover ACs closed (names). Fidelity: faithful + the ADR'd divergences (server-streaming vs bidi diagram; presence-not-aggregate) + any execution deviations, honestly.
3. Epic + CLAUDE.md + `make ci` + `make tla` green.
Commit: `docs: close out phase 2a — data-plane server delivered`

---

## Not in this plan (→ 2B, planned after 2A executes)

Python SDK (`sdks/python`), ring-4 conformance suite, scripted crash demo vs the SDK, benchmarks (incl. bidi-stream revisit data), crates.io publication (needs `manifest-cleanup-workspace-deps`), pool-size knob + relay/sink deadlock note, `tests/support` dedup across crates.
