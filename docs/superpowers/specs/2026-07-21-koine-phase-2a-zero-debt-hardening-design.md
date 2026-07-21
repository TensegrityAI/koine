# Koiné Phase 2A Zero-Debt Hardening — Design Specification

- **Date:** 2026-07-21
- **Authors:** Marcos Saez (Kael) & Codex
- **Status:** Approved by the maintainer on 2026-07-21
- **Amends:** the delivery semantics, CI, testing, and phase-gate details in
  [the original Koiné design](2026-07-16-koine-design.md)
- **Decision records:** accepted ADR-0016 and ADR-0017

---

## 1. Purpose and closure policy

Phase 2A has working domain, application, Postgres, gRPC, and server slices,
but a completed implementation is not a closed phase while a known safety or
reliability gap remains. This hardening pass establishes the closure rule:

> Phase 2B remains blocked until every gap attributable to phases 0, 1, and 2A
> is either closed with evidence or shown to belong to an explicitly future
> capability.

The target deployment for this specification is deliberately narrow:

- one `koine-server` process;
- one in-process outbox relay;
- one Postgres database;
- the `koine.v1` worker data plane already implemented in phase 2A.

Python SDK work, ring-4 language conformance, benchmarks, REST, MCP, full
observability, dashboard behavior, and multi-server/HA coordination remain in
their declared future phases. They are not made implicit requirements of this
hardening pass.

## 2. Evidence that motivates the work

The phase-2A audit found a strong baseline: full CI and the current TLA+ model
pass; leases, heartbeats, acknowledgements, recovery, and Postgres integration
are exercised by real tests. It also found four classes of closure gap:

1. `Dispatcher::expired` discovers job IDs separately from the later event
   append. A heartbeat can extend the ephemeral deadline between those steps,
   while optimistic stream versioning cannot detect it because heartbeats do
   not append events.
2. Every idle Postgres fetch currently creates its own `PgListener` through
   the shared pool. Best-effort presence can also wait on a saturated pool,
   delaying a correctness-critical request even though its error is ignored.
3. Pool sizing and several duration values rely on silent or zero-permitting
   configuration.
4. CI contains floating action/tool references; internal dependency metadata,
   publication intent, and public phase documentation are not yet reconciled
   with executable reality.

These are phase-closure defects, not phase-2B features.

## 3. Required invariants

The implementation and its tests must preserve all existing guarantees and add
the following invariants:

1. **Heartbeat fence:** once a heartbeat successfully extends a live lease, an
   expiry decision made against the prior deadline cannot retire that lease.
2. **Single retirement:** a lease grant produces at most one accepted
   `LeaseExpired` transition.
3. **Atomic truth:** selecting an expired lease, deriving its events, appending
   them, and updating the dispatch projection is one adapter transaction or
   critical section.
4. **No hidden request dependency:** wakeup and presence optimizations cannot
   exhaust or indefinitely delay the pool used by append, dispatch, heartbeat,
   and acknowledgement.
5. **Fail-closed configuration:** zero or malformed safety-relevant values are
   rejected before the server begins accepting traffic.
6. **Reproducible executable inputs:** CI actions, downloaded executables, and
   code-generation tools have immutable identities and integrity checks where
   the ecosystem permits them.
7. **Truthful phase surface:** documentation and publication metadata
   distinguish implemented behavior from planned behavior.

## 4. Atomic lease retirement

### 4.1 Port contract

The split `expired(now) -> Vec<JobId>` discovery contract is removed. The
`Dispatcher` port instead exposes a composite operation conceptually equivalent
to:

```text
retire_next_expired_lease() -> Result<Option<JobId>, DispatchError>
```

The exact Rust name may be refined in the implementation plan, but its contract
may not be weakened: the driven adapter owns candidate selection, deadline
validation, domain transition, event append, and projection update as one
atomic operation. The sweep use case loops this operation until it returns
`None`; it no longer loads and appends a previously discovered job itself.

The operation continues to derive `LeaseExpired` and any retry/parking events
through the `Job` aggregate. It does not synthesize state transitions in SQL.
Event kinds and the public `koine.v1` wire contract do not change.

### 4.2 Postgres serialization

The Postgres adapter performs one transaction per retired lease:

1. Begin a transaction and obtain the operation's current time only after a
   database connection has been acquired.
2. Select one row whose lease deadline has passed, using
   `FOR UPDATE SKIP LOCKED` and deterministic ordering.
3. Re-evaluate the expiry predicate on the locked/current row rather than
   trusting an earlier candidate snapshot.
4. Load and rehydrate the event stream inside the transaction.
5. Ask the aggregate to expire the active lease and derive all resulting
   events.
6. Append those events and update the synchronous dispatch projection through
   the existing transaction-local append machinery.
7. Commit, then report the retired job ID.

`extend_lease` must validate the matching lease ID and current deadline in the
same row-modifying statement that acquires the row lock. The serialization
outcomes are therefore exhaustive:

- if heartbeat wins, retirement skips or observes the extended deadline;
- if retirement wins, the dispatch row no longer represents that live lease
  and heartbeat returns `false`;
- unrelated jobs remain concurrent through `SKIP LOCKED`.

An adapter/backend error rolls the transaction back. The sweep reports the
error and never treats a partial operation as successful.

### 4.3 In-memory equivalence

The in-memory dispatcher implements the same contract under its existing
shared state lock. Time is read and the candidate is chosen only while holding
that lock. Ring-2 tests remain a semantic contract suite, not a weaker test
double.

### 4.4 TLA+ reconvergence

The formal model gains explicit time, a lease deadline, a lease identity or
generation, and a `Heartbeat` action. `Heartbeat` is enabled only for the
matching live, unexpired lease. `Expire` is enabled only after the current
deadline and retires that exact grant.

The model checks at least:

- no job has two active lease grants;
- an accepted heartbeat fences expiry based on the prior deadline;
- a lease grant is retired at most once;
- late acknowledgements remain conflicts rather than silent success;
- if time advances and heartbeats eventually stop, a non-terminal leased job
  eventually becomes eligible, parked, or terminal.

Liveness is conditional on heartbeats stopping. A worker that renews forever
legitimately keeps its lease, so the model must not claim unconditional
eventual settlement. Bounds such as a finite heartbeat count are modeling
devices and must be stated in the model comments and architecture wiki.

### 4.5 Verification

TDD begins with regressions that deterministically drive both lock orders:

- heartbeat commits first, retirement must not append `LeaseExpired`;
- retirement commits first, heartbeat must return `false`;
- concurrent sweepers retire one grant once;
- the in-memory adapter produces the same outcomes;
- TLC explores heartbeat, tick, expiry, acknowledgement, and retry
  interleavings without invariant or deadlock violations.

## 5. Postgres resource safety

### 5.1 Explicit pool contract

`connect_pool` takes an explicit configuration and uses `PgPoolOptions`.
`koine-server serve` exposes:

- `KOINE_DB_MAX_CONNECTIONS`, default `16`, greater than zero;
- `KOINE_DB_ACQUIRE_TIMEOUT_MS`, default `5000`, greater than zero.

The values are parsed before connecting, included in `.env.example`, and
documented in the server and Postgres architecture pages. The server's total
steady-state connection budget is the configured operational pool plus one
dedicated `LISTEN` connection.

A pool size of one remains legal for tests and constrained deployments because
the listener is separate and no operation may acquire a second connection
while holding the first. Capacity guidance describes the latency trade-off
rather than inventing an unproven universal minimum.

### 5.2 One notification listener per process

`PgSignal` owns one dedicated Postgres listener, established before the gRPC
server accepts traffic. It subscribes once to `koine_dispatch` and fans payloads
out through a bounded in-process broadcast channel.

Each `wait(queue, timeout)` subscribes to that fan-out, ignores other queues,
and returns when its queue arrives or its timeout expires. A lagged/closed
receiver returns promptly so the fetch loop rechecks dispatch state. The
existing `idle_poll` remains the correctness backstop for notifications that
race ahead of subscription or are lost during reconnect.

The listener is not borrowed from the operational pool. Transient listener
failure must reconnect with bounded backoff rather than permanently killing the
notification task. Initial connection/listen failure prevents startup because
silently launching a degraded server is not an acceptable configuration
success.

A pressure test creates many idle waiters and proves that they use one listener
connection and do not prevent an append/lease operation from acquiring the
operational pool.

### 5.3 Presence is best-effort in latency

ADR-0015 already makes worker presence best-effort in outcome. The Postgres
adapter makes it best-effort in latency too:

- attempt non-blocking pool acquisition;
- skip the presence write immediately when the pool is saturated;
- bound an acquired write to a documented `100 ms` budget;
- continue swallowing the resulting presence-only error.

No `Fetch`, `Heartbeat`, `Start`, `Succeed`, or `Fail` response may wait for the
pool's general acquisition timeout solely to record presence. A saturation test
holds every operational connection and asserts that `seen()` returns within a
generous test bound around the declared budget.

### 5.4 Configuration validation

Startup also rejects zero values for:

- `KOINE_MAX_LEASE_TTL_MS`;
- `KOINE_IDLE_POLL_MS`;
- `KOINE_DB_MAX_CONNECTIONS`;
- `KOINE_DB_ACQUIRE_TIMEOUT_MS`.

Malformed addresses, durations, sizes, database URLs, and missing/empty worker
tokens retain their existing fail-closed behavior. Unit tests cover defaults,
explicit valid values, zero, malformed input, and boundary conversion.

## 6. Reproducible build and supply-chain policy

Proposed ADR-0017 records the tooling decision. A new repository policy and CI
gate enforce these rules:

1. GitHub Actions use full commit SHAs with a trailing human-readable release
   comment. Major tags, branches, and `latest` are forbidden.
2. Downloaded executables use a versioned URL plus an in-repository expected
   SHA-256. The TLA+ tools jar follows the already established gitleaks pattern.
3. Cargo-installed CI tools use exact versions and `--locked`.
4. Node tooling is installed through a committed `package-lock.json` and
   `npm ci`; CI and `make md` execute the locked `markdownlint-cli2`.
5. Container images used by repository-owned development/test definitions use
   an immutable digest where the consumer supports it. Any unsupported case is
   named in the policy with its narrower version pin and rationale.
6. Provider-managed infrastructure that cannot be content-addressed, such as a
   GitHub-hosted runner label, is an explicit exception; the label itself is
   version-specific rather than `ubuntu-latest`.

The gate scans repository-owned CI and download surfaces for prohibited
floating forms. It is deliberately narrow enough not to mistake prose or
historical immutable plans for executable configuration.

### Hermetic protobuf compilation

`koine-proto` adopts an exact `protoc-bin-vendored` build dependency. Its build
script sets the compiler path before tonic/prost code generation. This removes
the floating `apt install protobuf-compiler` step and makes local, CI, and
packaged builds select the compiler through Cargo's locked, checksummed
dependency graph.

The accepted package version and supported targets are verified during
implementation. Unsupported targets fail with an explicit build error; the
build script does not silently fall back to an arbitrary system `protoc`.

## 7. Manifest and publication hardening

Internal crates are declared once in root `[workspace.dependencies]` with both
`path` and `version`; every consumer uses `workspace = true`. External
dependencies are not mechanically centralized as part of this cut because that
would create unrelated policy and feature-unification churn.

All package descriptions lose literal Markdown backticks. Dependency edges are
captured from normalized `cargo metadata` before and after the rewrite and must
remain identical.

Every workspace crate is explicitly `publish = false` while phase 2B is
blocked. A future publication task must opt approved crates in individually;
empty future-phase adapters can never be published accidentally as apparently
complete packages.

For each implemented crate, `cargo package --allow-dirty --list` verifies that
the package boundary contains every required source, proto, migration, license,
and build input. This is packaging evidence, not authorization to publish.

## 8. Documentation and lifecycle truth

The same hardening program updates:

- `README.md` to separate behavior available today from explicitly planned
  repair/resume, REST, MCP, CLI, and dashboard capabilities;
- `ROADMAP.md`, the phase-2 epic, and `CLAUDE.md` to say "2A implementation
  complete; closure hardening active; 2B blocked";
- `.env.example` with every serve variable, default, and non-zero constraint;
- the architecture pages for application, Postgres, gRPC, server, proto, and
  formal models where their contracts change;
- the original approved design with dated links to accepted ADR-0016,
  ADR-0017, and this amendment.

The implementation plan creates review-sized backlog items for atomic lease
retirement, Postgres resource safety, and operational closure. Existing todo
items for pool sizing, CI pinning, and manifest cleanup remain the owners of
their original acceptance criteria and close only when their evidence and
spec-fidelity sections are complete.

Accepted ADR text is not rewritten. When ADR-0016 is accepted, ADR-0011 gains
a dated clarification that only decision 0011(c)'s split expiry mechanism is
superseded; 0011(a) and 0011(b) remain accepted. ADR-0008 remains accepted and
is refined, not superseded.

## 9. Delivery slices and failure handling

Implementation proceeds in independently reviewable TDD slices:

1. atomic lease-retirement contract, in-memory implementation, Postgres
   transaction, regressions, and TLA+ model;
2. explicit pool/configuration, dedicated listener fan-out, presence budget,
   pressure tests, and architecture pages;
3. supply-chain gate, hermetic protobuf tooling, manifests, packaging, public
   documentation, and backlog reconciliation;
4. final cross-cutting product exercise and phase-closure audit.

A slice that exposes a new phase-0/1/2A defect does not close with a vague
follow-up. The defect is fixed inside the owning slice when bounded; otherwise
it becomes a ready, blocking item and 2B remains locked. A failure attributable
solely to an explicitly future capability is documented against that future
phase and does not counterfeit present completeness.

No database schema rewrite, event mutation, wire-contract change, REST/MCP
implementation, full observability platform, SDK work, benchmark-driven tuning,
or HA coordinator is authorized by this specification.

## 10. Exit gate

Phase 2A hardening is complete only when all of the following are fresh and
recorded:

1. Every acceptance criterion in the hardening and legacy todo items passes by
   its declared method.
2. Deterministic in-memory and real-Postgres concurrency regressions pass.
3. `make tla` checks the heartbeat-aware protocol without invariant, liveness,
   or deadlock errors under the documented bounds.
4. `make ci` passes in full.
5. A real `koine-server serve` + Postgres + gRPC worker flow demonstrates
   lease, heartbeat, expiry/recovery, and clean shutdown behavior.
6. Package-boundary checks pass for every implemented crate; no publish command
   is executed.
7. The supply-chain scan reports no unapproved floating executable input.
8. Architecture wiki, public docs, ADR links, backlog evidence, and
   spec-fidelity statements agree with code.
9. Review produces both required verdicts: spec compliance and code quality.
10. The live backlog contains no unresolved item attributable to phase 0, 1,
    or 2A.

Only after this gate may `CLAUDE.md` and the phase-2 epic name phase 2B as the
active planning target.

## 11. Alternatives rejected

### Recheck a previously discovered job before appending expiry

Rejected because the recheck remains separate from the ephemeral heartbeat
write. Event-stream optimistic concurrency cannot fence a change that creates
no event.

### Add `LeaseExtended` events

Rejected because heartbeat-rate event volume contradicts the accepted
ephemeral-heartbeat design. Atomic adapter serialization provides the required
fence without changing the event taxonomy.

### Merge `Dispatcher` and `EventStore`

Rejected because it creates a god-port and broadens crate contracts beyond the
one composite operation that needs transaction ownership.

### Keep one Postgres listener per idle fetch

Rejected because connection consumption then scales with idle workers rather
than broker instances and competes with the correctness-critical hot path.

### Keep system `protoc` and only document it

Rejected because documentation does not make CI/local/package code generation
reproducible or integrity-controlled.

### Check generated Rust protobuf files into source control

Rejected because it introduces a second representation that can drift from the
first-class `.proto` contract and still requires a trusted regeneration path.

### Perform only the three existing backlog cleanups

Rejected because it would leave the heartbeat race, listener pool pressure,
zero-value configuration, stale phase claims, and drift-prone supply-chain
surfaces outside the closure gate.

## 2026-07-21 applicable supply-chain audit amendment

Post-implementation audit makes `markdownlint-cli2` 0.23.1, Node 22.23.1,
npm 10.9.8, and
`actions/setup-node@a0853c24544627f65ddf259abe73b1d18a591444` (`v5.0.0`)
the applicable reviewed identities. The setup action disables package-manager
cache, installation remains `npm ci --ignore-scripts`, and execution remains
`npm exec`. The fail-closed gate and its mutation fixtures enforce the
approved action/comment allowlist, download checksums, npm/Node identities,
and image policy. These exact upgrades preserve ADR-0017 rather than changing
the accepted design.
