# Koiné Phase 1A — Domain Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Koiné's event-sourced core — the `Job` aggregate, full v1 event taxonomy, retry policy, application ports and use cases, and a complete in-memory store — so the entire job lifecycle (enqueue→lease→ack/fail→retry→park, plus crash recovery via lease expiry and late-ack conflicts) runs and is proven by test rings 1–2, with no external dependencies.

**Architecture:** Pure domain in `koine-domain` (no async, no I/O — state is a fold over events); driven ports and use cases in `koine-application` (native async-fn-in-trait, generic static dispatch); `koine-store-memory` implements every port and hosts the ring-2 lifecycle tests (avoiding a dev-dep cycle). The dispatch index is maintained synchronously by the store's `append` (the ADR 0006 contract, exercised in memory exactly as Postgres will in 1B); the atomic claim-and-lease composite lives behind the `Dispatcher` port (ADR 0011). Phase 1B adds the Postgres adapters, outbox relay, and ring-3 tests against real migrations.

**Tech Stack:** Rust 1.95 (edition 2024), serde/serde_json, uuid (v7), chrono, thiserror 2, proptest (ring 1), tokio (dev, ring 2).

**Reference:** design spec §3 (`docs/superpowers/specs/2026-07-16-koine-design.md`), epic `.apptlas/epics/phase-1-event-sourced-core.md`. Plan 1A covers epic items 1–7 + 12-partial; 1B covers items 8–11 + 13.

## Global Constraints

- Workspace lints are law: `unsafe_code = forbid`, clippy `all`+`pedantic`, `missing_docs` — CI runs clippy `-D warnings`, so every public item in plan code gets a `///` doc line and pedantic fallout is fixed minimally, never `#[allow]`ed without a stated constraint.
- No `unwrap()`/`expect()` outside `#[cfg(test)]` and `tests/` (workspace lint warns; CI denies).
- `koine-domain` MUST NOT gain async, I/O, or infra deps. Allowed pure-data deps (recorded in ADR 0010): `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`.
- Dependency direction (compile-enforced): application → domain; store-memory → application + domain. No new edges beyond these (they already exist from phase 0).
- **The event log is append-only truth** (AGENTS.md): no mutation of recorded events; corrections are new events. Every event envelope carries `correlation_id`, `causation_id` (optional), and W3C `traceparent` (optional) — spec §3.
- Event kind strings are the serde `snake_case` tags and are wire/storage contract — never rename after commit.
- TDD per task: failing test → minimal code → green → commit. Conventional Commits, subject ≤72 chars after `type: `.
- `make ci` green before every commit (hooks enforce fmt/typos on commit and clippy/test on push).
- DoD applies at plan close (Task 13): wiki pages for touched crates, spec-fidelity statement in the backlog item, non-implementer review per task (subagent-driven flow provides it).

## File map

| File | Responsibility |
| --- | --- |
| `crates/koine-domain/src/lib.rs` | Module wiring + re-exports |
| `crates/koine-domain/src/error.rs` | `DomainError` |
| `crates/koine-domain/src/ids.rs` | `JobId`, `EventId`, `LeaseId`, `CorrelationId`, `WorkerId` |
| `crates/koine-domain/src/queue.rs` | `QueueName` (validated), `Priority` |
| `crates/koine-domain/src/retry.rs` | `RetryPolicy`, `RetryDecision`, deterministic full-jitter backoff |
| `crates/koine-domain/src/events.rs` | `JobEvent` (v1 + reserved kinds), `JobError`, `ParkReason`, `ReportedOutcome`, `EventEnvelope`, `SCHEMA_VERSION` |
| `crates/koine-domain/src/job.rs` | `JobState`, `Job` aggregate: `from_events`, `apply`, command methods |
| `crates/koine-domain/tests/state_machine_props.rs` | Ring-1 proptest invariants |
| `crates/koine-application/src/lib.rs` | Module wiring |
| `crates/koine-application/src/ports.rs` | `EventStore`, `Dispatcher`, `Clock`, `IdGenerator`, port errors, `LeasedJob` |
| `crates/koine-application/src/lineage.rs` | `Lineage` + `EnvelopeFactory` helper |
| `crates/koine-application/src/use_cases/*.rs` | `enqueue`, `worker_ack` (start/succeed/fail), `lease`, `heartbeat`, `sweep`, `cancel` |
| `crates/koine-store-memory/src/lib.rs` | Module wiring |
| `crates/koine-store-memory/src/store.rs` | `InMemoryEventStore` + synchronous dispatch index |
| `crates/koine-store-memory/src/dispatcher.rs` | `InMemoryDispatcher` |
| `crates/koine-store-memory/src/test_support.rs` | `FixedClock` (steppable), `SeededIds` |
| `crates/koine-store-memory/tests/lifecycle.rs` | Ring-2 lifecycle + crash/late-ack scenarios |
| `docs/adr/0010…0011` | Event encoding & identity; dispatch atomicity & lease ephemera |
| `docs/formal/lease_protocol.tla` (+README) | TLA+ skeleton co-evolving with the state machine |
| `docs/architecture/*.md` | Wiki pages (Task 13) |

---

### Task 1: ADRs 0010–0011 and crate dependencies

**Files:**
- Create: `docs/adr/0010-event-encoding-and-identity.md`, `docs/adr/0011-dispatch-atomicity-and-lease-ephemera.md`
- Modify: `docs/adr/INDEX.md` (two rows), `crates/koine-domain/Cargo.toml`, `crates/koine-application/Cargo.toml`, `crates/koine-store-memory/Cargo.toml`

**Interfaces:**
- Consumes: ADR template `docs/adr/template.md`; adr-workflow.
- Produces: the decisions every later task's code embodies; dependency sections later tasks compile against.

- [ ] **Step 1: Write ADR 0010**

```markdown
# 0010 — Event encoding and identity

- **Status:** accepted
- **Date:** 2026-07-18
- **Context:** Phase 1 must fix how events are represented in Rust, serialized,
  and identified — the costliest decisions to change later (spec §3, epic
  risk #1). Requirements: stable wire/storage encoding, additive evolution,
  time-ordered ids for index locality, full lineage on every event.
- **Decision:**
  - Ids are **UUIDv7** (time-ordered) newtypes: `JobId`, `EventId`, `LeaseId`,
    `CorrelationId`. `WorkerId` is a validated string (workers name
    themselves). Generation happens only behind the application `IdGenerator`
    port — `koine-domain` stays free of clocks and randomness.
  - Events are one Rust enum `JobEvent`, serde **internally tagged**
    (`#[serde(tag = "type", rename_all = "snake_case")]`). The snake_case tag
    IS the canonical event-kind string, exposed as `JobEvent::kind()`;
    adapters store it in an indexed column derived from the same source.
  - The envelope (`EventEnvelope`) carries: `event_id`, `stream_id` (= job
    id), `version` (1-based, per stream), `recorded_at`, `correlation_id`,
    `causation_id: Option<EventId>`, `traceparent: Option<String>` (W3C),
    `schema_version: u16` (`SCHEMA_VERSION = 1`), `event`.
  - Evolution is **additive only**: new fields get `#[serde(default)]`;
    renames/removals require a new event kind. `schema_version` bumps only on
    envelope-shape changes.
  - Reserved durable-execution kinds (checkpoint, signal, approval, suspend,
    resume, repair — spec §3) are defined in the enum from day one with
    minimal but real transition semantics; no v1 command produces them.
  - `koine-domain` allowed pure-data deps: serde, serde_json, uuid, chrono,
    thiserror. Anything with I/O, time sources, or randomness stays out.
- **Consequences:** stable contract for 1B's Postgres columns and phase 2's
  proto mapping; the tag string is unrenamable forever; internally-tagged
  serde forbids non-object payload shapes (acceptable: all payloads are
  objects); UUIDv7 leaks coarse creation time in ids (acceptable for jobs).
- **Alternatives considered:** externally tagged serde (uglier JSON, tag
  duplicated per adapter); event structs per kind + registry (more types, no
  exhaustiveness checking); UUIDv4 (index churn on append-heavy tables);
  integer sequence ids in domain (couples identity to storage).
```

- [ ] **Step 2: Write ADR 0011**

```markdown
# 0011 — Dispatch atomicity and lease ephemera

- **Status:** accepted
- **Date:** 2026-07-18
- **Context:** ADR 0006 makes the dispatch projection transactional with the
  event append. Phase 1 must fix *which component guarantees* the two
  composite operations that cannot be split: (a) append(events) + dispatch
  index update; (b) claim-eligible-job + append(JobLeased). And spec §3 makes
  heartbeats ephemeral — lease extension must not write events.
- **Decision:**
  - **(a) is the `EventStore::append` contract:** every adapter updates the
    dispatch index synchronously and atomically with the append, reacting to
    event kinds (enqueued/retry_scheduled → eligible; leased → claimed;
    succeeded/parked/cancelled/suspended → removed; lease_expired/failed →
    eligible now). The in-memory store implements this contract exactly as
    Postgres will (single transaction) so ring-2 tests exercise real
    semantics.
  - **(b) is the `Dispatcher::lease_next` contract:** the adapter atomically
    selects the highest-priority eligible job (priority DESC, then
    enqueue order; `not_before <= now`), produces `JobLeased` **via the
    domain aggregate** (domain validation stays authoritative — adapters may
    depend on domain), appends it, updates the index, and returns the
    `LeasedJob`. Use cases stay thin over this port; the orchestration
    atomicity lives where the transaction lives.
  - **Lease extension is ephemeral:** `Dispatcher::extend_lease` updates the
    lease deadline in the dispatch index only. No event is written. Lease
    *expiry* is an event (`lease_expired`), produced by the sweep use case
    from `Dispatcher::expired`.
- **Consequences:** the dispatch index is rebuildable from the log (it is a
  projection), but is the only component allowed to hold ephemeral lease
  deadlines; adapters carry more responsibility and get contract tests;
  `Dispatcher` adapters need `IdGenerator`+`Clock` injected.
- **Alternatives considered:** two-phase claim-then-append in the use case
  (crash between = claimed-but-unrecorded limbo); `LeaseExtended` events
  (heartbeat-rate log spam, contradicts spec §3); merging Dispatcher into
  EventStore (one god-port, harder to keep honest).
```

- [ ] **Step 3: Add INDEX rows**

Append to the table in `docs/adr/INDEX.md`:

```markdown
| [0010](0010-event-encoding-and-identity.md) | Event encoding and identity | accepted | 2026-07-18 |
| [0011](0011-dispatch-atomicity-and-lease-ephemera.md) | Dispatch atomicity and lease ephemera | accepted | 2026-07-18 |
```

- [ ] **Step 4: Declare dependencies**

`crates/koine-domain/Cargo.toml` — replace the empty `[dependencies]` and add dev-deps:

```toml
[dependencies]
chrono = { version = "0.4", features = ["serde"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }

[dev-dependencies]
proptest = "1"
```

`crates/koine-application/Cargo.toml` — under `[dependencies]` keep the existing `koine-domain` line and add:

```toml
chrono = { version = "0.4", features = ["serde"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }
```

`crates/koine-store-memory/Cargo.toml` — keep existing internal deps and add:

```toml
chrono = { version = "0.4", features = ["serde"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt"] }
```

- [ ] **Step 5: Verify and commit**

Run: `make ci`
Expected: `✓ all CI checks green` (markdownlint covers the ADRs; nothing compiles differently yet).

```bash
git add docs/adr/ crates/koine-domain/Cargo.toml crates/koine-application/Cargo.toml crates/koine-store-memory/Cargo.toml Cargo.lock
git commit -m "docs: accept ADRs 0010-0011 and declare phase-1a dependencies"
```

---

### Task 2: Domain foundations — errors, ids, queue names

**Files:**
- Create: `crates/koine-domain/src/error.rs`, `crates/koine-domain/src/ids.rs`, `crates/koine-domain/src/queue.rs`
- Modify: `crates/koine-domain/src/lib.rs`

**Interfaces:**
- Consumes: nothing (first code task).
- Produces: `DomainError` (variants below), `JobId`/`EventId`/`LeaseId`/`CorrelationId` (UUID newtypes with `new(Uuid)`, `as_uuid()`), `WorkerId::new(impl Into<String>) -> Result<WorkerId, DomainError>` + `as_str()`, `QueueName::new(...) -> Result<QueueName, DomainError>` + `as_str()`, `Priority(pub i16)` with `Default` = 0. All serde-serializable; ids `#[serde(transparent)]`, `QueueName` validated on deserialize via `try_from`.

- [ ] **Step 1: Write the failing tests (inline `#[cfg(test)]` in each file — shown here together)**

```rust
// in queue.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_queue_names() {
        for name in ["default", "emails.high", "a", "x_1-2.z"] {
            assert!(QueueName::new(name).is_ok(), "{name} should be valid");
        }
    }

    #[test]
    fn rejects_invalid_queue_names() {
        let too_long = "q".repeat(129);
        for name in ["", "UPPER", "with space", "ünïcode", too_long.as_str()] {
            assert!(QueueName::new(name).is_err(), "{name:?} should be invalid");
        }
    }

    #[test]
    fn queue_name_deserialization_revalidates() {
        assert!(serde_json::from_str::<QueueName>("\"ok-queue\"").is_ok());
        assert!(serde_json::from_str::<QueueName>("\"NOT OK\"").is_err());
    }

    #[test]
    fn priority_defaults_to_zero() {
        assert_eq!(Priority::default(), Priority(0));
    }
}

// in ids.rs
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn job_id_serde_is_transparent() {
        let id = JobId::new(Uuid::from_u128(7));
        let json = serde_json::to_string(&id).expect("serialize");
        assert_eq!(json, format!("\"{}\"", id.as_uuid()));
        let back: JobId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, id);
    }

    #[test]
    fn worker_id_rejects_empty_and_oversized() {
        assert!(WorkerId::new("").is_err());
        assert!(WorkerId::new("w".repeat(257)).is_err());
        assert!(WorkerId::new("worker-1").is_ok());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-domain`
Expected: compile error (modules don't exist) — that counts as the failing state for scaffolding tasks.

- [ ] **Step 3: Implement `error.rs`**

```rust
//! Domain errors.

use thiserror::Error;

/// Errors produced by domain validation and state transitions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// The event is not applicable in the aggregate's current state.
    #[error("illegal transition: event `{event}` in state `{state}`")]
    IllegalTransition {
        /// State name at the time of the attempt.
        state: &'static str,
        /// Event kind that was rejected.
        event: &'static str,
    },
    /// A queue name failed validation.
    #[error("invalid queue name: {reason}")]
    InvalidQueueName {
        /// Which rule was violated.
        reason: &'static str,
    },
    /// A worker id failed validation.
    #[error("invalid worker id: {reason}")]
    InvalidWorkerId {
        /// Which rule was violated.
        reason: &'static str,
    },
    /// An event stream did not start with `enqueued`.
    #[error("event stream must start with `enqueued`, got `{got}`")]
    StreamMustStartWithEnqueued {
        /// Kind of the offending first event.
        got: &'static str,
    },
    /// The command references a lease the job does not currently hold.
    #[error("lease mismatch")]
    LeaseMismatch,
    /// A lease TTL could not be represented.
    #[error("ttl out of range")]
    InvalidTtl,
}
```

- [ ] **Step 4: Implement `ids.rs`**

```rust
//! Identifier newtypes. UUIDv7 by convention; generated only behind the
//! application `IdGenerator` port so the domain stays free of clocks and
//! randomness (ADR 0010).

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::DomainError;

macro_rules! uuid_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Wraps an existing UUID.
            #[must_use]
            pub const fn new(id: Uuid) -> Self {
                Self(id)
            }

            /// Returns the inner UUID.
            #[must_use]
            pub const fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

uuid_newtype!(
    /// Identity of a job — also its event-stream id.
    JobId
);
uuid_newtype!(
    /// Identity of a single recorded event.
    EventId
);
uuid_newtype!(
    /// Identity of one lease grant.
    LeaseId
);
uuid_newtype!(
    /// Correlates all events caused by one logical operation.
    CorrelationId
);

/// Identity a worker chooses for itself (validated string).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkerId(String);

impl WorkerId {
    /// Validates and wraps a worker id: non-empty, ≤256 bytes, no control chars.
    pub fn new(id: impl Into<String>) -> Result<Self, DomainError> {
        let id = id.into();
        if id.is_empty() {
            return Err(DomainError::InvalidWorkerId { reason: "empty" });
        }
        if id.len() > 256 {
            return Err(DomainError::InvalidWorkerId { reason: "longer than 256 bytes" });
        }
        if id.chars().any(char::is_control) {
            return Err(DomainError::InvalidWorkerId { reason: "control characters" });
        }
        Ok(Self(id))
    }

    /// The id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WorkerId {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<WorkerId> for String {
    fn from(value: WorkerId) -> Self {
        value.0
    }
}

impl fmt::Display for WorkerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
```

- [ ] **Step 5: Implement `queue.rs`**

```rust
//! Queue naming and dispatch priority.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// A validated queue name: 1–128 chars of `[a-z0-9._-]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct QueueName(String);

impl QueueName {
    /// Validates and wraps a queue name.
    pub fn new(name: impl Into<String>) -> Result<Self, DomainError> {
        let name = name.into();
        if name.is_empty() {
            return Err(DomainError::InvalidQueueName { reason: "empty" });
        }
        if name.len() > 128 {
            return Err(DomainError::InvalidQueueName { reason: "longer than 128 bytes" });
        }
        let ok = name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
        if !ok {
            return Err(DomainError::InvalidQueueName {
                reason: "only [a-z0-9._-] allowed",
            });
        }
        Ok(Self(name))
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for QueueName {
    type Error = DomainError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<QueueName> for String {
    fn from(value: QueueName) -> Self {
        value.0
    }
}

impl fmt::Display for QueueName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Dispatch priority: higher claims first; equal priorities are FIFO.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Priority(pub i16);
```

- [ ] **Step 6: Wire `lib.rs`**

Replace the body of `crates/koine-domain/src/lib.rs` (keep the existing `//!` doc line as the first line):

```rust
//! Koiné domain layer: aggregates, domain events, state machines. No I/O, no async, no infra deps.

pub mod error;
pub mod ids;
pub mod queue;

pub use error::DomainError;
pub use ids::{CorrelationId, EventId, JobId, LeaseId, WorkerId};
pub use queue::{Priority, QueueName};
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p koine-domain && cargo clippy -p koine-domain --all-targets -- -D warnings`
Expected: all tests PASS; clippy clean (fix any pedantic fallout minimally — e.g. `#[must_use]`, doc backticks).

- [ ] **Step 8: Commit**

```bash
git add crates/koine-domain
git commit -m "feat(domain): add ids, queue names, priority, and domain errors"
```

---

### Task 3: RetryPolicy — deterministic exponential backoff with full jitter

**Files:**
- Create: `crates/koine-domain/src/retry.rs`
- Modify: `crates/koine-domain/src/lib.rs` (add `pub mod retry;` + `pub use retry::{RetryDecision, RetryPolicy};`)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `RetryPolicy { max_attempts: u32, base_delay: Duration, max_delay: Duration }` (`Default` = 20 / 2s / 900s), `RetryPolicy::decide(&self, attempts_completed: u32, seed: u64) -> RetryDecision`, `RetryDecision::{RetryAfter(Duration), Park}`. Fully deterministic: equal inputs ⇒ equal outputs (spec §3 "exponential backoff + jitter" as a pure function — epic item 4).

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parks_when_attempts_exhausted() {
        let p = RetryPolicy { max_attempts: 3, ..RetryPolicy::default() };
        assert_eq!(p.decide(3, 42), RetryDecision::Park);
        assert_eq!(p.decide(4, 42), RetryDecision::Park);
    }

    #[test]
    fn is_deterministic_for_equal_inputs() {
        let p = RetryPolicy::default();
        assert_eq!(p.decide(2, 1234), p.decide(2, 1234));
    }

    #[test]
    fn different_seeds_can_differ() {
        let p = RetryPolicy::default();
        let outcomes: std::collections::HashSet<_> =
            (0..32u64).map(|s| format!("{:?}", p.decide(5, s))).collect();
        assert!(outcomes.len() > 1, "jitter must actually vary");
    }

    #[test]
    fn delay_never_exceeds_cap() {
        let p = RetryPolicy {
            max_attempts: 100,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
        };
        for attempt in 1..99 {
            for seed in 0..16u64 {
                if let RetryDecision::RetryAfter(d) = p.decide(attempt, seed) {
                    assert!(d <= Duration::from_secs(30), "attempt {attempt} seed {seed}: {d:?}");
                }
            }
        }
    }

    #[test]
    fn first_attempt_delay_is_within_base() {
        let p = RetryPolicy::default();
        if let RetryDecision::RetryAfter(d) = p.decide(1, 99) {
            assert!(d <= p.base_delay);
        } else {
            panic!("attempt 1 of 20 must retry");
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-domain retry`
Expected: compile error (`retry` module missing).

- [ ] **Step 3: Implement `retry.rs`**

```rust
//! Retry policy: deterministic exponential backoff with full jitter (spec §3).
//!
//! Pure function of (policy, attempt, seed) — the seed comes from the
//! application's `IdGenerator` port so the domain stays deterministic.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What happens after a failed attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetryDecision {
    /// Retry after the given delay.
    RetryAfter(Duration),
    /// Attempts exhausted — park the job.
    Park,
}

/// Exponential backoff with full jitter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum failed attempts before parking (attempts are 1-based).
    pub max_attempts: u32,
    /// Backoff base: the cap for the first retry's delay.
    pub base_delay: Duration,
    /// Upper cap for any computed delay.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 20,
            base_delay: Duration::from_secs(2),
            max_delay: Duration::from_secs(900),
        }
    }
}

impl RetryPolicy {
    /// Decision after `attempts_completed` failed attempts (≥1).
    ///
    /// Full jitter: the delay is uniform in `[0, min(base * 2^(n-1), cap)]`,
    /// driven entirely by `seed` — equal inputs give equal outputs.
    #[must_use]
    pub fn decide(&self, attempts_completed: u32, seed: u64) -> RetryDecision {
        if attempts_completed >= self.max_attempts {
            return RetryDecision::Park;
        }
        let exp = attempts_completed.saturating_sub(1).min(31);
        let uncapped = self.base_delay.saturating_mul(2_u32.saturating_pow(exp));
        let capped = uncapped.min(self.max_delay);
        let millis = u64::try_from(capped.as_millis()).unwrap_or(u64::MAX);
        let jittered = if millis == 0 {
            0
        } else {
            splitmix64(seed ^ u64::from(attempts_completed)) % (millis + 1)
        };
        RetryDecision::RetryAfter(Duration::from_millis(jittered))
    }
}

/// `SplitMix64` — tiny, well-distributed, dependency-free PRNG step.
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p koine-domain retry && cargo clippy -p koine-domain --all-targets -- -D warnings`
Expected: 5 tests PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/koine-domain
git commit -m "feat(domain): add deterministic retry policy with full jitter"
```

---

### Task 4: Event taxonomy and envelope

**Files:**
- Create: `crates/koine-domain/src/events.rs`
- Modify: `crates/koine-domain/src/lib.rs` (add `pub mod events;` + `pub use events::{EventEnvelope, JobError, JobEvent, ParkReason, ReportedOutcome, SCHEMA_VERSION};`)

**Interfaces:**
- Consumes: `QueueName`, `Priority`, `RetryPolicy`, ids (Tasks 2–3).
- Produces: `JobEvent` (all variants below), `JobEvent::kind(&self) -> &'static str` (== serde tag), `JobError { kind, message, stacktrace, retryable }`, `ParkReason::{RetriesExhausted, NonRetryableError, ApprovalDenied}`, `ReportedOutcome::{Succeeded, Failed}`, `EventEnvelope { event_id, stream_id, version, recorded_at, correlation_id, causation_id, traceparent, schema_version, event }`, `SCHEMA_VERSION: u16 = 1`.

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ids::*, queue::*, retry::RetryPolicy};
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn sample_events() -> Vec<JobEvent> {
        let worker = WorkerId::new("w1").expect("valid");
        let lease = LeaseId::new(Uuid::from_u128(2));
        let t = Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("valid ts");
        vec![
            JobEvent::Enqueued {
                queue: QueueName::new("default").expect("valid"),
                payload: serde_json::json!({"n": 1}),
                priority: Priority(5),
                retry_policy: RetryPolicy::default(),
                not_before: None,
            },
            JobEvent::Leased { worker: worker.clone(), lease, expires_at: t },
            JobEvent::Started { worker: worker.clone() },
            JobEvent::Succeeded { result: Some(serde_json::json!("ok")) },
            JobEvent::Failed {
                error: JobError {
                    kind: "io".into(),
                    message: "boom".into(),
                    stacktrace: None,
                    retryable: true,
                },
                attempt: 1,
            },
            JobEvent::LeaseExpired { lease },
            JobEvent::RetryScheduled { attempt: 2, not_before: t },
            JobEvent::Parked { reason: ParkReason::RetriesExhausted },
            JobEvent::Cancelled { reason: Some("operator".into()) },
            JobEvent::LateAckConflict {
                worker,
                lease,
                reported: ReportedOutcome::Succeeded,
            },
            JobEvent::CheckpointRecorded { key: "step-1".into(), data: serde_json::json!(1) },
            JobEvent::SignalReceived { name: "resume".into(), data: serde_json::json!(null) },
            JobEvent::ApprovalRequested { key: "deploy".into() },
            JobEvent::ApprovalGranted { key: "deploy".into(), approver: "kael".into() },
            JobEvent::ApprovalDenied { key: "deploy".into(), approver: "kael".into() },
            JobEvent::Suspended,
            JobEvent::Resumed,
            JobEvent::Repaired { new_payload: None, note: "fixed input".into() },
        ]
    }

    #[test]
    fn serde_tag_matches_kind_for_every_variant() {
        for ev in sample_events() {
            let json = serde_json::to_value(&ev).expect("serialize");
            assert_eq!(json["type"], ev.kind(), "tag/kind drift on {ev:?}");
            let back: JobEvent = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, ev);
        }
    }

    #[test]
    fn envelope_round_trips() {
        let env = EventEnvelope {
            event_id: EventId::new(Uuid::from_u128(10)),
            stream_id: JobId::new(Uuid::from_u128(11)),
            version: 1,
            recorded_at: Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"),
            correlation_id: CorrelationId::new(Uuid::from_u128(12)),
            causation_id: None,
            traceparent: Some("00-11111111111111111111111111111111-2222222222222222-01".into()),
            schema_version: SCHEMA_VERSION,
            event: JobEvent::Suspended,
        };
        let json = serde_json::to_string(&env).expect("serialize");
        let back: EventEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, env);
    }

    #[test]
    fn envelope_deserializes_without_optional_lineage() {
        // additive-evolution guarantee: absent optional fields default.
        let json = serde_json::json!({
            "event_id": Uuid::from_u128(1),
            "stream_id": Uuid::from_u128(2),
            "version": 1,
            "recorded_at": "2026-07-18T12:00:00Z",
            "correlation_id": Uuid::from_u128(3),
            "schema_version": 1,
            "event": {"type": "suspended"}
        });
        let env: EventEnvelope = serde_json::from_value(json).expect("deserialize");
        assert_eq!(env.causation_id, None);
        assert_eq!(env.traceparent, None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-domain events`
Expected: compile error (`events` module missing).

- [ ] **Step 3: Implement `events.rs`**

```rust
//! The v1 event taxonomy (spec §3) plus reserved durable-execution kinds,
//! and the storage envelope (ADR 0010). Kind strings are wire/storage
//! contract — never rename.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ids::{CorrelationId, EventId, JobId, LeaseId, WorkerId},
    queue::{Priority, QueueName},
    retry::RetryPolicy,
};

/// Envelope schema version (bumped only on envelope-shape changes).
pub const SCHEMA_VERSION: u16 = 1;

/// Structured failure info reported by a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobError {
    /// Machine-readable error class (worker-defined).
    pub kind: String,
    /// Human-readable message.
    pub message: String,
    /// Optional stacktrace.
    #[serde(default)]
    pub stacktrace: Option<String>,
    /// Whether retrying can plausibly succeed.
    pub retryable: bool,
}

/// Why a job was parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParkReason {
    /// The retry policy's attempts were exhausted.
    RetriesExhausted,
    /// The worker reported the error as not retryable.
    NonRetryableError,
    /// A human denied a requested approval (reserved — phase 5).
    ApprovalDenied,
}

/// Outcome a late-acking worker reported after its lease had expired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedOutcome {
    /// The worker claims the job succeeded.
    Succeeded,
    /// The worker claims the job failed.
    Failed,
}

/// Everything that can happen to a job. Serde-internally-tagged: the
/// `snake_case` tag is the canonical kind string (`kind()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    /// A new job entered the system (always version 1 of its stream).
    Enqueued {
        /// Destination queue.
        queue: QueueName,
        /// Opaque worker payload.
        payload: Value,
        /// Dispatch priority.
        priority: Priority,
        /// Retry policy fixed at enqueue time.
        retry_policy: RetryPolicy,
        /// Earliest dispatch time (`None` = immediately eligible).
        #[serde(default)]
        not_before: Option<DateTime<Utc>>,
    },
    /// A worker acquired a lease.
    Leased {
        /// The claiming worker.
        worker: WorkerId,
        /// The lease grant.
        lease: LeaseId,
        /// Deadline unless extended by heartbeats (ephemeral extensions —
        /// ADR 0011).
        expires_at: DateTime<Utc>,
    },
    /// The worker began executing.
    Started {
        /// The executing worker.
        worker: WorkerId,
    },
    /// Terminal success.
    Succeeded {
        /// Optional worker-reported result.
        #[serde(default)]
        result: Option<Value>,
    },
    /// An attempt failed (retry decision follows as its own event).
    Failed {
        /// Structured error.
        error: JobError,
        /// The attempt number that failed (1-based).
        attempt: u32,
    },
    /// The lease deadline passed without an ack — the worker is presumed dead.
    LeaseExpired {
        /// The expired lease.
        lease: LeaseId,
    },
    /// A retry was scheduled with backoff.
    RetryScheduled {
        /// The attempt count already completed.
        attempt: u32,
        /// Earliest re-dispatch time.
        not_before: DateTime<Utc>,
    },
    /// Dead but repairable, with full history retained.
    Parked {
        /// Why.
        reason: ParkReason,
    },
    /// Terminal cancellation by an operator or agent.
    Cancelled {
        /// Optional reason.
        #[serde(default)]
        reason: Option<String>,
    },
    /// An ack arrived after the lease expired — recorded, never discarded
    /// (spec §3: information is never lost).
    LateAckConflict {
        /// The late worker.
        worker: WorkerId,
        /// The lease it thought it held.
        lease: LeaseId,
        /// What it reported.
        reported: ReportedOutcome,
    },
    /// Reserved (phase 5): a journaled side-effect result.
    CheckpointRecorded {
        /// Step key (unique per job).
        key: String,
        /// Journaled result.
        data: Value,
    },
    /// Reserved (phase 5): external signal delivered to the job.
    SignalReceived {
        /// Signal name.
        name: String,
        /// Signal payload.
        data: Value,
    },
    /// Reserved (phase 5): human-in-the-loop approval requested.
    ApprovalRequested {
        /// Approval key.
        key: String,
    },
    /// Reserved (phase 5): approval granted.
    ApprovalGranted {
        /// Approval key.
        key: String,
        /// Who granted it.
        approver: String,
    },
    /// Reserved (phase 5): approval denied.
    ApprovalDenied {
        /// Approval key.
        key: String,
        /// Who denied it.
        approver: String,
    },
    /// Reserved (phase 5): execution suspended.
    Suspended,
    /// Reserved (phase 5): execution resumed.
    Resumed,
    /// Reserved (phase 5): operator repaired the job; it continues from its
    /// last checkpoint with history preserved.
    Repaired {
        /// Replacement payload, if the input was the problem.
        #[serde(default)]
        new_payload: Option<Value>,
        /// Operator note.
        note: String,
    },
}

impl JobEvent {
    /// Canonical kind string — identical to the serde tag.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Enqueued { .. } => "enqueued",
            Self::Leased { .. } => "leased",
            Self::Started { .. } => "started",
            Self::Succeeded { .. } => "succeeded",
            Self::Failed { .. } => "failed",
            Self::LeaseExpired { .. } => "lease_expired",
            Self::RetryScheduled { .. } => "retry_scheduled",
            Self::Parked { .. } => "parked",
            Self::Cancelled { .. } => "cancelled",
            Self::LateAckConflict { .. } => "late_ack_conflict",
            Self::CheckpointRecorded { .. } => "checkpoint_recorded",
            Self::SignalReceived { .. } => "signal_received",
            Self::ApprovalRequested { .. } => "approval_requested",
            Self::ApprovalGranted { .. } => "approval_granted",
            Self::ApprovalDenied { .. } => "approval_denied",
            Self::Suspended => "suspended",
            Self::Resumed => "resumed",
            Self::Repaired { .. } => "repaired",
        }
    }
}

/// The unit of storage and truth: one event plus its lineage (ADR 0010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// This event's identity.
    pub event_id: EventId,
    /// The job (= stream) this belongs to.
    pub stream_id: JobId,
    /// 1-based position in the stream (optimistic-concurrency token).
    pub version: u64,
    /// Broker-side record time.
    pub recorded_at: DateTime<Utc>,
    /// Correlates all events of one logical operation.
    pub correlation_id: CorrelationId,
    /// The event that caused this one, if known.
    #[serde(default)]
    pub causation_id: Option<EventId>,
    /// W3C trace context of the causing operation.
    #[serde(default)]
    pub traceparent: Option<String>,
    /// Envelope schema version.
    pub schema_version: u16,
    /// The event itself.
    pub event: JobEvent,
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p koine-domain events && cargo clippy -p koine-domain --all-targets -- -D warnings`
Expected: 3 tests PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/koine-domain
git commit -m "feat(domain): add v1 event taxonomy, reserved kinds, and envelope"
```

---

### Task 5: `Job` aggregate — states, fold, transition table

**Files:**
- Create: `crates/koine-domain/src/job.rs`
- Modify: `crates/koine-domain/src/error.rs` (one new variant), `crates/koine-domain/src/lib.rs` (add `pub mod job;` + `pub use job::{Job, JobState};`)

**Interfaces:**
- Consumes: everything from Tasks 2–4.
- Produces: `JobState` (variants below) with `pub const fn name(&self) -> &'static str`; `Job { pub id: JobId, pub queue: QueueName, pub priority: Priority, pub payload: Value, pub retry_policy: RetryPolicy, pub attempt: u32, pub state: JobState, pub version: u64 }`; `Job::initial_event(queue, payload, priority, retry_policy, not_before) -> JobEvent`; `Job::from_events(&[EventEnvelope]) -> Result<Job, DomainError>`; `Job::apply(&mut self, &JobEvent) -> Result<(), DomainError>`. Task 6 adds the command methods on the same type.

- [ ] **Step 1: Add the new error variant to `error.rs`**

```rust
    /// Envelope versions were not sequential when folding a stream.
    #[error("non-sequential version: expected {expected}, got {got}")]
    NonSequentialVersion {
        /// The version the fold expected next.
        expected: u64,
        /// The version found on the envelope.
        got: u64,
    },
```

- [ ] **Step 2: Write the failing tests (inline in `job.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::*, ids::*, queue::*, retry::RetryPolicy};
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts")
    }

    fn env(version: u64, event: JobEvent) -> EventEnvelope {
        EventEnvelope {
            event_id: EventId::new(Uuid::from_u128(u128::from(version))),
            stream_id: JobId::new(Uuid::from_u128(1)),
            version,
            recorded_at: ts(),
            correlation_id: CorrelationId::new(Uuid::from_u128(9)),
            causation_id: None,
            traceparent: None,
            schema_version: SCHEMA_VERSION,
            event,
        }
    }

    fn enqueued() -> JobEvent {
        Job::initial_event(
            QueueName::new("default").expect("q"),
            serde_json::json!({"k": 1}),
            Priority(3),
            RetryPolicy::default(),
            None,
        )
    }

    fn worker() -> WorkerId {
        WorkerId::new("w1").expect("w")
    }

    fn lease_id() -> LeaseId {
        LeaseId::new(Uuid::from_u128(50))
    }

    #[test]
    fn folds_the_happy_path() {
        let job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
            env(3, JobEvent::Started { worker: worker() }),
            env(4, JobEvent::Succeeded { result: None }),
        ])
        .expect("fold");
        assert_eq!(job.state, JobState::Succeeded);
        assert_eq!(job.version, 4);
        assert_eq!(job.attempt, 0);
    }

    #[test]
    fn stream_must_start_with_enqueued() {
        let err = Job::from_events(&[env(1, JobEvent::Suspended)]).expect_err("must fail");
        assert_eq!(err, DomainError::StreamMustStartWithEnqueued { got: "suspended" });
    }

    #[test]
    fn rejects_non_sequential_versions() {
        let err = Job::from_events(&[env(1, enqueued()), env(3, JobEvent::Suspended)])
            .expect_err("must fail");
        assert_eq!(err, DomainError::NonSequentialVersion { expected: 2, got: 3 });
    }

    #[test]
    fn started_is_illegal_while_pending() {
        let mut job = Job::from_events(&[env(1, enqueued())]).expect("fold");
        let err = job.apply(&JobEvent::Started { worker: worker() }).expect_err("illegal");
        assert_eq!(err, DomainError::IllegalTransition { state: "pending", event: "started" });
        assert_eq!(job.version, 1, "version must not advance on rejection");
    }

    #[test]
    fn failed_records_attempt_and_returns_to_pending() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
            env(3, JobEvent::Started { worker: worker() }),
        ])
        .expect("fold");
        job.apply(&JobEvent::Failed {
            error: JobError { kind: "k".into(), message: "m".into(), stacktrace: None, retryable: true },
            attempt: 1,
        })
        .expect("apply failed");
        assert_eq!(job.attempt, 1);
        assert_eq!(job.state, JobState::Pending { not_before: None });
        job.apply(&JobEvent::RetryScheduled { attempt: 1, not_before: ts() }).expect("apply retry");
        assert_eq!(job.state, JobState::Pending { not_before: Some(ts()) });
    }

    #[test]
    fn lease_expiry_increments_attempt() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
        ])
        .expect("fold");
        job.apply(&JobEvent::LeaseExpired { lease: lease_id() }).expect("apply");
        assert_eq!(job.attempt, 1);
        assert_eq!(job.state, JobState::Pending { not_before: None });
    }

    #[test]
    fn terminal_states_absorb_everything_except_late_ack() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Cancelled { reason: None }),
        ])
        .expect("fold");
        assert!(job.apply(&JobEvent::Suspended).is_err());
        job.apply(&JobEvent::LateAckConflict {
            worker: worker(),
            lease: lease_id(),
            reported: ReportedOutcome::Failed,
        })
        .expect("late ack is a pure record");
        assert_eq!(job.state, JobState::Cancelled);
        assert_eq!(job.version, 3);
    }

    #[test]
    fn repair_resets_attempts_and_replaces_payload() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
            env(3, JobEvent::LeaseExpired { lease: lease_id() }),
            env(4, JobEvent::Parked { reason: ParkReason::RetriesExhausted }),
        ])
        .expect("fold");
        job.apply(&JobEvent::Repaired {
            new_payload: Some(serde_json::json!({"k": 2})),
            note: "fixed".into(),
        })
        .expect("repair");
        assert_eq!(job.attempt, 0);
        assert_eq!(job.payload, serde_json::json!({"k": 2}));
        assert_eq!(job.state, JobState::Pending { not_before: None });
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p koine-domain job`
Expected: compile error (`job` module missing).

- [ ] **Step 4: Implement `job.rs` (states, fold, apply)**

```rust
//! The `Job` aggregate: state is a fold over events (ADR 0004). Commands
//! validate; `apply` trusts recorded history and only rejects transitions
//! that could never have been legal.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{
    error::DomainError,
    events::{EventEnvelope, JobEvent, ParkReason},
    ids::{JobId, LeaseId, WorkerId},
    queue::{Priority, QueueName},
    retry::RetryPolicy,
};

/// Resting states of a job (spec §3 state machine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Eligible (or scheduled) for dispatch.
    Pending {
        /// Earliest dispatch time; `None` = eligible now.
        not_before: Option<DateTime<Utc>>,
    },
    /// Claimed by a worker, not yet started.
    Leased {
        /// Holder.
        worker: WorkerId,
        /// The grant.
        lease: LeaseId,
        /// Recorded deadline (ephemeral extensions live in the dispatch
        /// index — ADR 0011).
        expires_at: DateTime<Utc>,
    },
    /// Executing.
    Running {
        /// Holder.
        worker: WorkerId,
        /// The grant.
        lease: LeaseId,
        /// Recorded deadline.
        expires_at: DateTime<Utc>,
    },
    /// Terminal success.
    Succeeded,
    /// Dead but repairable (full history retained).
    Parked {
        /// Why.
        reason: ParkReason,
    },
    /// Terminal cancellation.
    Cancelled,
    /// Reserved (phase 5).
    Suspended,
    /// Reserved (phase 5).
    AwaitingApproval {
        /// Approval key being waited on.
        key: String,
    },
}

impl JobState {
    /// Stable state name for diagnostics and error messages.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Pending { .. } => "pending",
            Self::Leased { .. } => "leased",
            Self::Running { .. } => "running",
            Self::Succeeded => "succeeded",
            Self::Parked { .. } => "parked",
            Self::Cancelled => "cancelled",
            Self::Suspended => "suspended",
            Self::AwaitingApproval { .. } => "awaiting_approval",
        }
    }
}

/// The aggregate. Public fields: the aggregate is a value; invariants are
/// guarded by `from_events`/`apply`, not by hiding.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// Identity (= stream id).
    pub id: JobId,
    /// Destination queue.
    pub queue: QueueName,
    /// Dispatch priority.
    pub priority: Priority,
    /// Opaque worker payload.
    pub payload: Value,
    /// Retry policy fixed at enqueue (until repaired).
    pub retry_policy: RetryPolicy,
    /// Completed (failed or expired) attempts.
    pub attempt: u32,
    /// Current state.
    pub state: JobState,
    /// Last applied stream version.
    pub version: u64,
}

impl Job {
    /// The stream-opening event for a new job.
    #[must_use]
    pub const fn initial_event(
        queue: QueueName,
        payload: Value,
        priority: Priority,
        retry_policy: RetryPolicy,
        not_before: Option<DateTime<Utc>>,
    ) -> JobEvent {
        JobEvent::Enqueued { queue, payload, priority, retry_policy, not_before }
    }

    /// Rebuilds a job by folding its recorded stream.
    pub fn from_events(envelopes: &[EventEnvelope]) -> Result<Self, DomainError> {
        let Some((first, rest)) = envelopes.split_first() else {
            return Err(DomainError::StreamMustStartWithEnqueued { got: "nothing" });
        };
        let JobEvent::Enqueued { queue, payload, priority, retry_policy, not_before } =
            &first.event
        else {
            return Err(DomainError::StreamMustStartWithEnqueued { got: first.event.kind() });
        };
        if first.version != 1 {
            return Err(DomainError::NonSequentialVersion { expected: 1, got: first.version });
        }
        let mut job = Self {
            id: first.stream_id,
            queue: queue.clone(),
            priority: *priority,
            payload: payload.clone(),
            retry_policy: retry_policy.clone(),
            attempt: 0,
            state: JobState::Pending { not_before: *not_before },
            version: 1,
        };
        for envelope in rest {
            if envelope.version != job.version + 1 {
                return Err(DomainError::NonSequentialVersion {
                    expected: job.version + 1,
                    got: envelope.version,
                });
            }
            job.apply(&envelope.event)?;
        }
        Ok(job)
    }

    /// Applies one event, advancing the state machine. Rejections leave the
    /// aggregate untouched (version included).
    pub fn apply(&mut self, event: &JobEvent) -> Result<(), DomainError> {
        use JobEvent as E;
        use JobState as S;
        let current = self.state.clone();
        let next = match (current, event) {
            (S::Pending { .. }, E::Leased { worker, lease, expires_at }) => S::Leased {
                worker: worker.clone(),
                lease: *lease,
                expires_at: *expires_at,
            },
            (S::Leased { worker, lease, expires_at }, E::Started { .. }) => {
                S::Running { worker, lease, expires_at }
            }
            (S::Running { .. }, E::Succeeded { .. }) => S::Succeeded,
            (S::Running { .. }, E::Failed { attempt, .. }) => {
                self.attempt = *attempt;
                S::Pending { not_before: None }
            }
            (S::Pending { .. }, E::RetryScheduled { attempt, not_before }) => {
                self.attempt = *attempt;
                S::Pending { not_before: Some(*not_before) }
            }
            (S::Leased { .. } | S::Running { .. }, E::LeaseExpired { .. }) => {
                self.attempt += 1;
                S::Pending { not_before: None }
            }
            (S::Pending { .. }, E::Parked { reason }) => S::Parked { reason: *reason },
            (
                S::Pending { .. }
                | S::Leased { .. }
                | S::Running { .. }
                | S::Suspended
                | S::AwaitingApproval { .. }
                | S::Parked { .. },
                E::Cancelled { .. },
            ) => S::Cancelled,
            // A late ack is a pure record: legal in every state, changes none.
            (state, E::LateAckConflict { .. }) => state,
            (state @ S::Running { .. }, E::CheckpointRecorded { .. }) => state,
            (
                state @ (S::Pending { .. }
                | S::Leased { .. }
                | S::Running { .. }
                | S::Suspended
                | S::AwaitingApproval { .. }),
                E::SignalReceived { .. },
            ) => state,
            // Stall-threshold crossings are informational records (spec §3);
            // produced by phase 2's heartbeat mechanics.
            (state @ (S::Leased { .. } | S::Running { .. }), E::Stalled) => state,
            (S::Running { .. }, E::ApprovalRequested { key }) => {
                S::AwaitingApproval { key: key.clone() }
            }
            (S::AwaitingApproval { .. }, E::ApprovalGranted { .. }) => {
                S::Pending { not_before: None }
            }
            (S::AwaitingApproval { .. }, E::ApprovalDenied { .. }) => {
                S::Parked { reason: ParkReason::ApprovalDenied }
            }
            (S::Pending { .. } | S::Running { .. }, E::Suspended) => S::Suspended,
            (S::Suspended, E::Resumed) => S::Pending { not_before: None },
            (S::Parked { .. }, E::Repaired { new_payload, .. }) => {
                if let Some(payload) = new_payload {
                    self.payload = payload.clone();
                }
                self.attempt = 0;
                S::Pending { not_before: None }
            }
            (state, ev) => {
                let rejected = DomainError::IllegalTransition {
                    state: state.name(),
                    event: ev.kind(),
                };
                self.state = state;
                return Err(rejected);
            }
        };
        self.state = next;
        self.version += 1;
        Ok(())
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p koine-domain job && cargo clippy -p koine-domain --all-targets -- -D warnings`
Expected: 8 tests PASS; clippy clean. (If clippy flags the large `apply` match, that is the transition table — it stays one function; add `#[allow(clippy::too_many_lines)]` on `apply` with the comment `// transition table: one function on purpose`.)

- [ ] **Step 6: Commit**

```bash
git add crates/koine-domain
git commit -m "feat(domain): add job aggregate with fold and transition table"
```

---

### Task 6: `Job` command methods

**Files:**
- Modify: `crates/koine-domain/src/job.rs`

**Interfaces:**
- Consumes: Task 5's `Job`/`JobState`.
- Produces (all on `impl Job`): `lease(&self, worker: WorkerId, lease: LeaseId, now: DateTime<Utc>, ttl: std::time::Duration) -> Result<JobEvent, DomainError>`; `start(&self, worker: &WorkerId) -> Result<JobEvent, DomainError>`; `succeed(&self, lease: LeaseId, result: Option<Value>) -> Result<JobEvent, DomainError>`; `fail(&self, lease: LeaseId, error: JobError, now: DateTime<Utc>, seed: u64) -> Result<Vec<JobEvent>, DomainError>`; `expire_lease(&self, now: DateTime<Utc>, seed: u64) -> Result<Vec<JobEvent>, DomainError>`; `cancel(&self, reason: Option<String>) -> Result<JobEvent, DomainError>`; and associated `late_ack(worker: WorkerId, lease: LeaseId, reported: ReportedOutcome) -> JobEvent`.

- [ ] **Step 1: Write the failing tests (append to `job.rs` tests module)**

```rust
    fn running_job() -> Job {
        Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
            env(3, JobEvent::Started { worker: worker() }),
        ])
        .expect("fold")
    }

    #[test]
    fn lease_requires_eligibility() {
        let job = Job::from_events(&[env(1, enqueued())]).expect("fold");
        let ev = job
            .lease(worker(), lease_id(), ts(), std::time::Duration::from_secs(30))
            .expect("eligible");
        assert!(matches!(ev, JobEvent::Leased { .. }));

        let future = ts() + chrono::TimeDelta::hours(1);
        let scheduled = Job::from_events(&[env(
            1,
            Job::initial_event(
                QueueName::new("default").expect("q"),
                serde_json::json!({}),
                Priority(0),
                RetryPolicy::default(),
                Some(future),
            ),
        )])
        .expect("fold");
        assert!(
            scheduled
                .lease(worker(), lease_id(), ts(), std::time::Duration::from_secs(30))
                .is_err(),
            "not_before in the future must refuse the lease"
        );
    }

    #[test]
    fn start_rejects_a_different_worker() {
        let job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
        ])
        .expect("fold");
        let other = WorkerId::new("intruder").expect("w");
        assert_eq!(job.start(&other).expect_err("mismatch"), DomainError::LeaseMismatch);
        assert!(job.start(&worker()).is_ok());
    }

    #[test]
    fn succeed_rejects_a_stale_lease() {
        let job = running_job();
        let stale = LeaseId::new(Uuid::from_u128(999));
        assert_eq!(job.succeed(stale, None).expect_err("stale"), DomainError::LeaseMismatch);
        assert!(job.succeed(lease_id(), None).is_ok());
    }

    #[test]
    fn fail_retryable_schedules_a_retry() {
        let job = running_job();
        let events = job
            .fail(
                lease_id(),
                JobError { kind: "k".into(), message: "m".into(), stacktrace: None, retryable: true },
                ts(),
                7,
            )
            .expect("fail");
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], JobEvent::Failed { attempt: 1, .. }));
        assert!(matches!(events[1], JobEvent::RetryScheduled { attempt: 1, .. }));
    }

    #[test]
    fn fail_non_retryable_parks() {
        let job = running_job();
        let events = job
            .fail(
                lease_id(),
                JobError { kind: "k".into(), message: "m".into(), stacktrace: None, retryable: false },
                ts(),
                7,
            )
            .expect("fail");
        assert!(matches!(events[1], JobEvent::Parked { reason: ParkReason::NonRetryableError }));
    }

    #[test]
    fn fail_at_exhaustion_parks() {
        let mut job = running_job();
        job.retry_policy = RetryPolicy { max_attempts: 1, ..RetryPolicy::default() };
        let events = job
            .fail(
                lease_id(),
                JobError { kind: "k".into(), message: "m".into(), stacktrace: None, retryable: true },
                ts(),
                7,
            )
            .expect("fail");
        assert!(matches!(events[1], JobEvent::Parked { reason: ParkReason::RetriesExhausted }));
    }

    #[test]
    fn expire_lease_emits_expiry_plus_retry_decision() {
        let job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
        ])
        .expect("fold");
        let events = job.expire_lease(ts(), 7).expect("expire");
        assert!(matches!(events[0], JobEvent::LeaseExpired { .. }));
        assert!(matches!(events[1], JobEvent::RetryScheduled { attempt: 1, .. }));
    }

    #[test]
    fn cancel_is_legal_until_terminal() {
        assert!(running_job().cancel(Some("op".into())).is_ok());
        let done = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Leased { worker: worker(), lease: lease_id(), expires_at: ts() }),
            env(3, JobEvent::Started { worker: worker() }),
            env(4, JobEvent::Succeeded { result: None }),
        ])
        .expect("fold");
        assert!(done.cancel(None).is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-domain job`
Expected: compile errors — the command methods don't exist yet.

- [ ] **Step 3: Implement the command methods (append to `impl Job` in `job.rs`)**

```rust
    /// Claims the job for a worker. Legal only while `Pending` and eligible
    /// (`not_before` reached).
    pub fn lease(
        &self,
        worker: WorkerId,
        lease: LeaseId,
        now: DateTime<Utc>,
        ttl: std::time::Duration,
    ) -> Result<JobEvent, DomainError> {
        match &self.state {
            JobState::Pending { not_before } if not_before.is_none_or(|t| t <= now) => {
                let ttl = chrono::TimeDelta::from_std(ttl).map_err(|_| DomainError::InvalidTtl)?;
                Ok(JobEvent::Leased { worker, lease, expires_at: now + ttl })
            }
            state => Err(DomainError::IllegalTransition { state: state.name(), event: "leased" }),
        }
    }

    /// Marks execution started. Legal only for the lease-holding worker.
    pub fn start(&self, worker: &WorkerId) -> Result<JobEvent, DomainError> {
        match &self.state {
            JobState::Leased { worker: holder, .. } if holder == worker => {
                Ok(JobEvent::Started { worker: worker.clone() })
            }
            JobState::Leased { .. } => Err(DomainError::LeaseMismatch),
            state => Err(DomainError::IllegalTransition { state: state.name(), event: "started" }),
        }
    }

    /// Acks success. Legal only while `Running` under the same lease.
    pub fn succeed(&self, lease: LeaseId, result: Option<Value>) -> Result<JobEvent, DomainError> {
        match &self.state {
            JobState::Running { lease: held, .. } if *held == lease => {
                Ok(JobEvent::Succeeded { result })
            }
            JobState::Running { .. } => Err(DomainError::LeaseMismatch),
            state => {
                Err(DomainError::IllegalTransition { state: state.name(), event: "succeeded" })
            }
        }
    }

    /// Acks failure: emits `failed` plus the retry decision (`retry_scheduled`
    /// or `parked`). Legal only while `Running` under the same lease.
    pub fn fail(
        &self,
        lease: LeaseId,
        error: JobError,
        now: DateTime<Utc>,
        seed: u64,
    ) -> Result<Vec<JobEvent>, DomainError> {
        match &self.state {
            JobState::Running { lease: held, .. } if *held == lease => {
                let attempt = self.attempt + 1;
                let retryable = error.retryable;
                let mut events = vec![JobEvent::Failed { error, attempt }];
                events.push(if retryable {
                    self.retry_decision_event(attempt, now, seed)?
                } else {
                    JobEvent::Parked { reason: ParkReason::NonRetryableError }
                });
                Ok(events)
            }
            JobState::Running { .. } => Err(DomainError::LeaseMismatch),
            state => Err(DomainError::IllegalTransition { state: state.name(), event: "failed" }),
        }
    }

    /// Records that the lease deadline passed: emits `lease_expired` plus the
    /// retry decision. Produced by the sweep, never by workers.
    pub fn expire_lease(
        &self,
        now: DateTime<Utc>,
        seed: u64,
    ) -> Result<Vec<JobEvent>, DomainError> {
        match &self.state {
            JobState::Leased { lease, .. } | JobState::Running { lease, .. } => {
                let attempt = self.attempt + 1;
                Ok(vec![
                    JobEvent::LeaseExpired { lease: *lease },
                    self.retry_decision_event(attempt, now, seed)?,
                ])
            }
            state => Err(DomainError::IllegalTransition {
                state: state.name(),
                event: "lease_expired",
            }),
        }
    }

    /// Cancels the job. Legal in every non-terminal state.
    pub fn cancel(&self, reason: Option<String>) -> Result<JobEvent, DomainError> {
        match &self.state {
            JobState::Succeeded | JobState::Cancelled => Err(DomainError::IllegalTransition {
                state: self.state.name(),
                event: "cancelled",
            }),
            _ => Ok(JobEvent::Cancelled { reason }),
        }
    }

    /// The conflict record for an ack that arrived after lease loss. Always
    /// applicable — information is never discarded (spec §3).
    #[must_use]
    pub const fn late_ack(
        worker: WorkerId,
        lease: LeaseId,
        reported: ReportedOutcome,
    ) -> JobEvent {
        JobEvent::LateAckConflict { worker, lease, reported }
    }

    fn retry_decision_event(
        &self,
        attempt: u32,
        now: DateTime<Utc>,
        seed: u64,
    ) -> Result<JobEvent, DomainError> {
        Ok(match self.retry_policy.decide(attempt, seed) {
            crate::retry::RetryDecision::RetryAfter(delay) => {
                let delay =
                    chrono::TimeDelta::from_std(delay).map_err(|_| DomainError::InvalidTtl)?;
                JobEvent::RetryScheduled { attempt, not_before: now + delay }
            }
            crate::retry::RetryDecision::Park => {
                JobEvent::Parked { reason: ParkReason::RetriesExhausted }
            }
        })
    }
```

Also add `ReportedOutcome` to the imports at the top of `job.rs`:
`use crate::events::{EventEnvelope, JobEvent, ParkReason, ReportedOutcome};`

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p koine-domain && cargo clippy -p koine-domain --all-targets -- -D warnings`
Expected: all domain tests PASS (16 in `job.rs` alone); clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/koine-domain
git commit -m "feat(domain): add job command methods with retry decisions"
```

---

### Task 7: Ring-1 property tests — the state machine cannot be corrupted

**Files:**
- Create: `crates/koine-domain/tests/state_machine_props.rs`

**Interfaces:**
- Consumes: the full public domain API (Tasks 2–6).
- Produces: the epic's non-negotiable invariant suite ("no event sequence may reach an illegal state") + the retry-cap property.

- [ ] **Step 1: Write the property tests**

```rust
//! Ring-1 property tests (testing-policy): whatever a client does through
//! commands, and whatever a store replays as events, the aggregate never
//! panics, never corrupts, and terminal states absorb.

use std::time::Duration;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use koine_domain::{
    DomainError, EventEnvelope, Job, JobError, JobEvent, JobState, LeaseId, ParkReason, Priority,
    QueueName, ReportedOutcome, RetryDecision, RetryPolicy, WorkerId, SCHEMA_VERSION,
};
use koine_domain::{CorrelationId, EventId, JobId};
use proptest::prelude::*;
use uuid::Uuid;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).single().expect("ts")
}

fn envelope(version: u64, event: JobEvent) -> EventEnvelope {
    EventEnvelope {
        event_id: EventId::new(Uuid::from_u128(1000 + u128::from(version))),
        stream_id: JobId::new(Uuid::from_u128(1)),
        version,
        recorded_at: t0(),
        correlation_id: CorrelationId::new(Uuid::from_u128(2)),
        causation_id: None,
        traceparent: None,
        schema_version: SCHEMA_VERSION,
        event,
    }
}

fn fresh_job() -> Job {
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(50),
    };
    let initial = Job::initial_event(
        QueueName::new("q").expect("q"),
        serde_json::json!({}),
        Priority(0),
        policy,
        None,
    );
    Job::from_events(&[envelope(1, initial)]).expect("initial fold")
}

#[derive(Debug, Clone)]
enum Cmd {
    Lease,
    Start,
    Succeed,
    FailRetryable,
    FailFatal,
    Expire,
    Cancel,
    Advance(u32),
}

fn cmd_strategy() -> impl Strategy<Value = Cmd> {
    prop_oneof![
        Just(Cmd::Lease),
        Just(Cmd::Start),
        Just(Cmd::Succeed),
        Just(Cmd::FailRetryable),
        Just(Cmd::FailFatal),
        Just(Cmd::Expire),
        Just(Cmd::Cancel),
        (0u32..7200).prop_map(Cmd::Advance),
    ]
}

fn job_error(retryable: bool) -> JobError {
    JobError { kind: "k".into(), message: "m".into(), stacktrace: None, retryable }
}

proptest! {
    /// Any interleaving of commands folds cleanly: commands only ever emit
    /// applicable events, attempt is monotonic, version counts applied
    /// events, terminal states absorb.
    #[test]
    fn command_sequences_never_corrupt(
        cmds in proptest::collection::vec(cmd_strategy(), 0..60),
        seed in any::<u64>(),
    ) {
        let mut job = fresh_job();
        let mut now = t0();
        let mut applied: u64 = 1;
        let worker = WorkerId::new("w").expect("w");
        let mut lease_counter: u128 = 100;
        let mut current_lease = LeaseId::new(Uuid::from_u128(lease_counter));

        for cmd in cmds {
            let events: Vec<JobEvent> = match cmd {
                Cmd::Advance(secs) => {
                    now += TimeDelta::seconds(i64::from(secs));
                    vec![]
                }
                Cmd::Lease => {
                    lease_counter += 1;
                    let lease = LeaseId::new(Uuid::from_u128(lease_counter));
                    match job.lease(worker.clone(), lease, now, Duration::from_secs(30)) {
                        Ok(ev) => {
                            current_lease = lease;
                            vec![ev]
                        }
                        Err(_) => vec![],
                    }
                }
                Cmd::Start => job.start(&worker).map(|ev| vec![ev]).unwrap_or_default(),
                Cmd::Succeed => {
                    job.succeed(current_lease, None).map(|ev| vec![ev]).unwrap_or_default()
                }
                Cmd::FailRetryable => {
                    job.fail(current_lease, job_error(true), now, seed).unwrap_or_default()
                }
                Cmd::FailFatal => {
                    job.fail(current_lease, job_error(false), now, seed).unwrap_or_default()
                }
                Cmd::Expire => job.expire_lease(now, seed).unwrap_or_default(),
                Cmd::Cancel => job.cancel(None).map(|ev| vec![ev]).unwrap_or_default(),
            };

            let attempt_before = job.attempt;
            for event in &events {
                job.apply(event).expect("commands only emit applicable events");
                applied += 1;
            }
            prop_assert!(job.attempt >= attempt_before, "attempt is monotonic");
            prop_assert_eq!(job.version, applied, "version counts applied events");

            if matches!(job.state, JobState::Succeeded | JobState::Cancelled) {
                prop_assert!(
                    job.lease(
                        worker.clone(),
                        LeaseId::new(Uuid::from_u128(999_999)),
                        now,
                        Duration::from_secs(1),
                    )
                    .is_err(),
                    "terminal states absorb commands"
                );
            }
        }
    }

    /// Replaying arbitrary (even nonsensical) event sequences never panics:
    /// each event either applies (version +1) or is rejected untouched.
    #[test]
    fn arbitrary_event_replay_never_panics(
        events in proptest::collection::vec(event_strategy(), 0..40),
    ) {
        let mut job = fresh_job();
        for event in events {
            let version_before = job.version;
            let state_before = job.state.clone();
            match job.apply(&event) {
                Ok(()) => prop_assert_eq!(job.version, version_before + 1),
                Err(DomainError::IllegalTransition { .. }) => {
                    prop_assert_eq!(job.version, version_before, "rejection must not advance");
                    prop_assert_eq!(job.state, state_before, "rejection must not mutate");
                }
                Err(other) => prop_assert!(false, "unexpected error: {other}"),
            }
        }
    }

    /// The retry policy's delay never exceeds its cap.
    #[test]
    fn retry_delay_respects_cap(attempt in 1u32..200, seed in any::<u64>()) {
        let policy = RetryPolicy {
            max_attempts: 200,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
        };
        if let RetryDecision::RetryAfter(delay) = policy.decide(attempt, seed) {
            prop_assert!(delay <= Duration::from_secs(60));
        }
    }
}

fn event_strategy() -> impl Strategy<Value = JobEvent> {
    let worker = WorkerId::new("w").expect("w");
    let lease = LeaseId::new(Uuid::from_u128(101));
    prop_oneof![
        Just(JobEvent::Leased { worker: worker.clone(), lease, expires_at: t0() }),
        Just(JobEvent::Started { worker: worker.clone() }),
        Just(JobEvent::Succeeded { result: None }),
        (1u32..5).prop_map(|attempt| JobEvent::Failed { error: job_error(true), attempt }),
        Just(JobEvent::LeaseExpired { lease }),
        (1u32..5).prop_map(|attempt| JobEvent::RetryScheduled { attempt, not_before: t0() }),
        Just(JobEvent::Parked { reason: ParkReason::RetriesExhausted }),
        Just(JobEvent::Cancelled { reason: None }),
        Just(JobEvent::LateAckConflict {
            worker,
            lease,
            reported: ReportedOutcome::Succeeded,
        }),
        Just(JobEvent::CheckpointRecorded { key: "s".into(), data: serde_json::json!(1) }),
        Just(JobEvent::ApprovalRequested { key: "a".into() }),
        Just(JobEvent::Suspended),
        Just(JobEvent::Resumed),
        Just(JobEvent::Repaired { new_payload: None, note: "n".into() }),
    ]
}
```

- [ ] **Step 2: Run the properties**

Run: `cargo test -p koine-domain --test state_machine_props`
Expected: 3 property tests PASS (256 cases each by default). If `command_sequences_never_corrupt` finds a counterexample, that is a REAL domain bug — fix the domain (Tasks 5–6), never weaken the property.

- [ ] **Step 3: Re-export check + full ring-1 gate**

`lib.rs` must re-export everything the test imports (`RetryDecision` included — add `pub use retry::{RetryDecision, RetryPolicy};` if Task 3 didn't). Run: `make ci`
Expected: `✓ all CI checks green`.

- [ ] **Step 4: Commit**

```bash
git add crates/koine-domain
git commit -m "test(domain): add state-machine property suite"
```

---

### Task 8: Application ports and envelope factory

**Files:**
- Create: `crates/koine-application/src/ports.rs`, `crates/koine-application/src/lineage.rs`
- Modify: `crates/koine-application/src/lib.rs`

**Interfaces:**
- Consumes: the domain API (Tasks 2–6).
- Produces: traits `EventStore` (`append(stream: JobId, expected_version: u64, envelopes: Vec<EventEnvelope>)`, `load(stream) -> Vec<EventEnvelope>` — both returning `impl Future<Output = Result<…, EventStoreError>> + Send`), `Dispatcher` (`lease_next(&QueueName, &WorkerId, ttl: Duration) -> Option<LeasedJob>`, `extend_lease(LeaseId, ttl) -> bool`, `expired(now) -> Vec<JobId>`), `Clock::now() -> DateTime<Utc>`, `IdGenerator::{job_id, event_id, lease_id, correlation_id, jitter_seed}`; structs `LeasedJob`, errors `EventStoreError::{VersionConflict{stream, expected}, StreamNotFound, Backend}`, `DispatchError::{Store(EventStoreError), Backend}`; `Lineage { correlation_id, causation_id, traceparent }` (all `Option`, `Default`) and `wrap_events(ids, clock, stream, base_version, correlation_id, causation_id, traceparent, events) -> Vec<EventEnvelope>`.

- [ ] **Step 1: Write the failing test (inline in `lineage.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Clock, IdGenerator};
    use chrono::{DateTime, TimeZone, Utc};
    use koine_domain::{CorrelationId, EventId, JobEvent, JobId, LeaseId};
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    struct TestIds(AtomicU64);
    impl TestIds {
        fn next(&self) -> Uuid {
            Uuid::from_u128(u128::from(self.0.fetch_add(1, Ordering::Relaxed)))
        }
    }
    impl IdGenerator for TestIds {
        fn job_id(&self) -> JobId { JobId::new(self.next()) }
        fn event_id(&self) -> EventId { EventId::new(self.next()) }
        fn lease_id(&self) -> LeaseId { LeaseId::new(self.next()) }
        fn correlation_id(&self) -> CorrelationId { CorrelationId::new(self.next()) }
        fn jitter_seed(&self) -> u64 { 7 }
    }

    struct TestClock;
    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts")
        }
    }

    #[test]
    fn wraps_events_with_sequential_versions_and_shared_lineage() {
        let ids = TestIds(AtomicU64::new(1));
        let clock = TestClock;
        let stream = JobId::new(Uuid::from_u128(500));
        let correlation = CorrelationId::new(Uuid::from_u128(600));
        let envelopes = wrap_events(
            &ids,
            &clock,
            stream,
            4,
            correlation,
            None,
            Some("00-abc-def-01".into()),
            vec![JobEvent::Suspended, JobEvent::Resumed],
        );
        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].version, 5);
        assert_eq!(envelopes[1].version, 6);
        assert_eq!(envelopes[0].correlation_id, correlation);
        assert_eq!(envelopes[1].traceparent.as_deref(), Some("00-abc-def-01"));
        assert_ne!(envelopes[0].event_id, envelopes[1].event_id);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p koine-application`
Expected: compile error (modules missing).

- [ ] **Step 3: Implement `ports.rs`**

```rust
//! Driven ports (spec §2). The composite-operation contracts come from
//! ADRs 0006 and 0011: adapters guarantee atomicity; use cases stay thin.

use std::future::Future;
use std::time::Duration;

use chrono::{DateTime, Utc};
use koine_domain::{
    CorrelationId, EventEnvelope, EventId, JobId, LeaseId, QueueName, WorkerId,
};
use serde_json::Value;
use thiserror::Error;

/// Errors from event-store adapters.
#[derive(Debug, Error)]
pub enum EventStoreError {
    /// Optimistic-concurrency conflict: the stream moved under the caller.
    #[error("version conflict on {stream}: expected {expected}")]
    VersionConflict {
        /// The stream that conflicted.
        stream: JobId,
        /// The version the caller expected to be current.
        expected: u64,
    },
    /// The stream does not exist.
    #[error("stream {0} not found")]
    StreamNotFound(JobId),
    /// Adapter/backend failure.
    #[error("backend: {0}")]
    Backend(String),
}

/// Errors from dispatcher adapters.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// A store operation inside the composite failed.
    #[error(transparent)]
    Store(#[from] EventStoreError),
    /// Adapter/backend failure.
    #[error("backend: {0}")]
    Backend(String),
}

/// Append-only event log.
///
/// Contract (ADR 0006 / 0011-a): `append` synchronously and atomically
/// updates the dispatch index as part of the same operation — an appended
/// `enqueued` is immediately claimable, an appended terminal event
/// immediately undispatchable.
pub trait EventStore: Send + Sync {
    /// Appends pre-versioned envelopes. `expected_version` is the stream's
    /// current last version (0 for a new stream); envelopes must continue
    /// it sequentially.
    fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> impl Future<Output = Result<(), EventStoreError>> + Send;

    /// Loads a full stream in version order.
    fn load(
        &self,
        stream: JobId,
    ) -> impl Future<Output = Result<Vec<EventEnvelope>, EventStoreError>> + Send;
}

/// A job handed to a worker after a successful claim.
#[derive(Debug, Clone, PartialEq)]
pub struct LeasedJob {
    /// The claimed job.
    pub job_id: JobId,
    /// Its queue.
    pub queue: QueueName,
    /// Opaque worker payload.
    pub payload: Value,
    /// Completed attempts before this lease (0 = first try).
    pub attempt: u32,
    /// The lease grant to ack against.
    pub lease: LeaseId,
    /// Deadline unless extended by heartbeats.
    pub expires_at: DateTime<Utc>,
    /// Correlation carried from the job's lineage.
    pub correlation_id: CorrelationId,
    /// Trace context carried from the job's lineage.
    pub traceparent: Option<String>,
}

/// Atomic claim plus ephemeral lease bookkeeping.
///
/// Contract (ADR 0011-b/c): `lease_next` atomically selects the
/// highest-priority eligible job (priority desc, then enqueue order,
/// `not_before <= now`), produces `leased` via the domain aggregate, appends
/// it, and updates the index — all one transaction. `extend_lease` touches
/// only the ephemeral deadline; no event is written.
pub trait Dispatcher: Send + Sync {
    /// Claims the next eligible job on `queue` for `worker`, or `None`.
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send;

    /// Extends a live lease's deadline. Returns `false` if the lease is
    /// unknown or already expired (the worker must stop working).
    fn extend_lease(
        &self,
        lease: LeaseId,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool, DispatchError>> + Send;

    /// Jobs whose lease deadline has passed as of `now` (sweep input).
    fn expired(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<JobId>, DispatchError>> + Send;
}

/// Time source.
pub trait Clock: Send + Sync {
    /// Current instant.
    fn now(&self) -> DateTime<Utc>;
}

/// Identity and randomness source (UUIDv7 in production adapters — ADR 0010).
pub trait IdGenerator: Send + Sync {
    /// New job id.
    fn job_id(&self) -> JobId;
    /// New event id.
    fn event_id(&self) -> EventId;
    /// New lease id.
    fn lease_id(&self) -> LeaseId;
    /// New correlation id.
    fn correlation_id(&self) -> CorrelationId;
    /// Seed for deterministic retry jitter.
    fn jitter_seed(&self) -> u64;
}
```

- [ ] **Step 4: Implement `lineage.rs`**

```rust
//! Envelope construction shared by all use cases.

use koine_domain::{
    CorrelationId, EventEnvelope, EventId, JobEvent, JobId, SCHEMA_VERSION,
};

use crate::ports::{Clock, IdGenerator};

/// Caller-supplied causal context for a command.
#[derive(Debug, Clone, Default)]
pub struct Lineage {
    /// Correlates this command's events with the caller's operation
    /// (`None` = the use case mints a fresh one where it starts a stream).
    pub correlation_id: Option<CorrelationId>,
    /// The event that caused this command, if any.
    pub causation_id: Option<EventId>,
    /// W3C trace context.
    pub traceparent: Option<String>,
}

/// Wraps domain events into envelopes with sequential versions after
/// `base_version`, one shared `recorded_at`/lineage, fresh event ids.
pub fn wrap_events<G, C>(
    ids: &G,
    clock: &C,
    stream: JobId,
    base_version: u64,
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    traceparent: Option<String>,
    events: Vec<JobEvent>,
) -> Vec<EventEnvelope>
where
    G: IdGenerator + ?Sized,
    C: Clock + ?Sized,
{
    let recorded_at = clock.now();
    let mut version = base_version;
    events
        .into_iter()
        .map(|event| {
            version += 1;
            EventEnvelope {
                event_id: ids.event_id(),
                stream_id: stream,
                version,
                recorded_at,
                correlation_id,
                causation_id,
                traceparent: traceparent.clone(),
                schema_version: SCHEMA_VERSION,
                event,
            }
        })
        .collect()
}
```

- [ ] **Step 5: Wire `lib.rs`**

Replace the body of `crates/koine-application/src/lib.rs` (keep the `//!` doc line and the `use koine_domain as _;` line is now obsolete — remove it, the crate uses the dep for real):

```rust
//! Koiné application layer: use cases and driven ports (`EventStore`, `OutboxRelay`, `ProjectionStore`, `LeaseManager`, `Clock`, `IdGenerator`).

pub mod lineage;
pub mod ports;

pub use lineage::{wrap_events, Lineage};
pub use ports::{
    Clock, DispatchError, Dispatcher, EventStore, EventStoreError, IdGenerator, LeasedJob,
};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p koine-application && cargo clippy -p koine-application --all-targets -- -D warnings`
Expected: 1 test PASS; clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/koine-application
git commit -m "feat(application): add driven ports and envelope factory"
```

---

### Task 9: In-memory event store with synchronous dispatch index

**Files:**
- Create: `crates/koine-store-memory/src/store.rs`, `crates/koine-store-memory/src/test_support.rs`
- Modify: `crates/koine-store-memory/src/lib.rs`

**Interfaces:**
- Consumes: `EventStore`/`EventStoreError` (Task 8), domain `Job::from_events` (Task 5).
- Produces: `InMemoryEventStore::new()` implementing `EventStore`; crate-internal `Inner { streams, index, seq }`, `DispatchEntry { queue, priority, seq, not_before, lease: Option<LeaseState> }`, `LeaseState { lease, worker, expires_at }`, and `InMemoryEventStore::{locked, append_locked, project_locked}` (used by Task 10's dispatcher); public test doubles `FixedClock::at(DateTime<Utc>)` + `advance(Duration)` (implements `Clock`) and `SeededIds::new(seed: u64)` (implements `IdGenerator`; ids are sequential `Uuid::from_u128(seed<<64 | counter)`, `jitter_seed()` returns the seed).

- [ ] **Step 1: Write the failing tests (inline in `store.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FixedClock, SeededIds};
    use chrono::{TimeZone, Utc};
    use koine_application::{ports::EventStore, wrap_events};
    use koine_domain::{Job, JobEvent, Priority, QueueName, RetryPolicy};

    fn clock() -> FixedClock {
        FixedClock::at(Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"))
    }

    fn enqueue_envelopes(
        ids: &SeededIds,
        clock: &FixedClock,
    ) -> (koine_domain::JobId, Vec<koine_domain::EventEnvelope>) {
        use koine_application::ports::IdGenerator;
        let stream = ids.job_id();
        let correlation = ids.correlation_id();
        let event = Job::initial_event(
            QueueName::new("default").expect("q"),
            serde_json::json!({"n": 1}),
            Priority(0),
            RetryPolicy::default(),
            None,
        );
        (stream, wrap_events(ids, clock, stream, 0, correlation, None, None, vec![event]))
    }

    #[tokio::test]
    async fn appends_and_loads_a_stream() {
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(1);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store.append(stream, 0, envelopes.clone()).await.expect("append");
        let loaded = store.load(stream).await.expect("load");
        assert_eq!(loaded, envelopes);
    }

    #[tokio::test]
    async fn rejects_version_conflicts() {
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(1);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store.append(stream, 0, envelopes.clone()).await.expect("append");
        let err = store.append(stream, 0, envelopes).await.expect_err("conflict");
        assert!(matches!(
            err,
            koine_application::EventStoreError::VersionConflict { expected: 0, .. }
        ));
    }

    #[tokio::test]
    async fn load_of_unknown_stream_is_not_found() {
        use koine_application::ports::IdGenerator;
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(2);
        let err = store.load(ids.job_id()).await.expect_err("missing");
        assert!(matches!(err, koine_application::EventStoreError::StreamNotFound(_)));
    }

    #[tokio::test]
    async fn append_maintains_the_dispatch_index() {
        use koine_application::ports::IdGenerator;
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(3);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store.append(stream, 0, envelopes).await.expect("append");
        {
            let inner = store.inner.lock().expect("lock");
            let entry = inner.index.get(&stream).expect("indexed after enqueue");
            assert!(entry.lease.is_none());
        }
        // cancel ⇒ removed from the index atomically with the append
        let correlation = ids.correlation_id();
        let cancel = wrap_events(
            &ids,
            &clock,
            stream,
            1,
            correlation,
            None,
            None,
            vec![JobEvent::Cancelled { reason: None }],
        );
        store.append(stream, 1, cancel).await.expect("append cancel");
        let inner = store.inner.lock().expect("lock");
        assert!(inner.index.get(&stream).is_none(), "terminal ⇒ undispatchable");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-store-memory`
Expected: compile error (modules missing).

- [ ] **Step 3: Implement `test_support.rs`**

```rust
//! Deterministic clock and id generator for rings 1–2 (and 1B's ring 3).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use koine_application::ports::{Clock, IdGenerator};
use koine_domain::{CorrelationId, EventId, JobId, LeaseId};
use uuid::Uuid;

/// Manually-advanced test clock.
pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    /// Starts the clock at `instant`.
    #[must_use]
    pub fn at(instant: DateTime<Utc>) -> Self {
        Self(Mutex::new(instant))
    }

    /// Moves time forward.
    pub fn advance(&self, by: Duration) {
        let delta = TimeDelta::from_std(by).unwrap_or(TimeDelta::MAX);
        match self.0.lock() {
            Ok(mut guard) => *guard += delta,
            Err(poisoned) => *poisoned.into_inner() += delta,
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        match self.0.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }
}

/// Sequential deterministic ids: `seed` in the high bits, a counter below.
pub struct SeededIds {
    seed: u64,
    counter: AtomicU64,
}

impl SeededIds {
    /// A generator whose ids embed `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed, counter: AtomicU64::new(1) }
    }

    fn next(&self) -> Uuid {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        Uuid::from_u128((u128::from(self.seed) << 64) | u128::from(n))
    }
}

impl IdGenerator for SeededIds {
    fn job_id(&self) -> JobId {
        JobId::new(self.next())
    }
    fn event_id(&self) -> EventId {
        EventId::new(self.next())
    }
    fn lease_id(&self) -> LeaseId {
        LeaseId::new(self.next())
    }
    fn correlation_id(&self) -> CorrelationId {
        CorrelationId::new(self.next())
    }
    fn jitter_seed(&self) -> u64 {
        self.seed
    }
}
```

- [ ] **Step 4: Implement `store.rs`**

```rust
//! In-memory `EventStore` honoring the ADR 0006/0011 contract exactly as the
//! Postgres adapter will: append and dispatch-index update are one atomic
//! step (here: one mutex hold; there: one transaction).

use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use koine_application::ports::{EventStore, EventStoreError};
use koine_domain::{
    EventEnvelope, Job, JobId, JobState, LeaseId, Priority, QueueName, WorkerId,
};

/// A live lease as the dispatch index sees it (deadline is ephemeral —
/// heartbeats move it without events, ADR 0011-c).
#[derive(Debug, Clone)]
pub(crate) struct LeaseState {
    pub(crate) lease: LeaseId,
    pub(crate) worker: WorkerId,
    pub(crate) expires_at: DateTime<Utc>,
}

/// One dispatchable (or leased) job in the index.
#[derive(Debug, Clone)]
pub(crate) struct DispatchEntry {
    pub(crate) queue: QueueName,
    pub(crate) priority: Priority,
    pub(crate) seq: u64,
    pub(crate) not_before: Option<DateTime<Utc>>,
    pub(crate) lease: Option<LeaseState>,
}

#[derive(Default)]
pub(crate) struct Inner {
    pub(crate) streams: HashMap<JobId, Vec<EventEnvelope>>,
    pub(crate) index: HashMap<JobId, DispatchEntry>,
    pub(crate) seq: u64,
}

/// In-memory event store plus dispatch index.
#[derive(Default)]
pub struct InMemoryEventStore {
    pub(crate) inner: Mutex<Inner>,
}

impl InMemoryEventStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn locked<T>(
        &self,
        f: impl FnOnce(&mut Inner) -> Result<T, EventStoreError>,
    ) -> Result<T, EventStoreError> {
        match self.inner.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(_) => Err(EventStoreError::Backend("store mutex poisoned".into())),
        }
    }

    pub(crate) fn append_locked(
        inner: &mut Inner,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> Result<(), EventStoreError> {
        let events = inner.streams.entry(stream).or_default();
        let current = u64::try_from(events.len())
            .map_err(|_| EventStoreError::Backend("stream too long".into()))?;
        if current != expected_version {
            return Err(EventStoreError::VersionConflict { stream, expected: expected_version });
        }
        let mut next = current;
        for envelope in &envelopes {
            next += 1;
            if envelope.version != next || envelope.stream_id != stream {
                return Err(EventStoreError::Backend(format!(
                    "malformed envelope batch for {stream}"
                )));
            }
        }
        events.extend(envelopes);
        let folded = Job::from_events(events)
            .map_err(|e| EventStoreError::Backend(format!("stream no longer folds: {e}")))?;
        Self::project_locked(inner, &folded);
        Ok(())
    }

    /// Re-derives the job's dispatch entry from its folded state — the index
    /// is a rebuildable projection, updated atomically with every append.
    pub(crate) fn project_locked(inner: &mut Inner, job: &Job) {
        let seq = inner.index.get(&job.id).map_or_else(
            || {
                inner.seq += 1;
                inner.seq
            },
            |entry| entry.seq,
        );
        match &job.state {
            JobState::Pending { not_before } => {
                inner.index.insert(
                    job.id,
                    DispatchEntry {
                        queue: job.queue.clone(),
                        priority: job.priority,
                        seq,
                        not_before: *not_before,
                        lease: None,
                    },
                );
            }
            JobState::Leased { worker, lease, expires_at }
            | JobState::Running { worker, lease, expires_at } => {
                inner.index.insert(
                    job.id,
                    DispatchEntry {
                        queue: job.queue.clone(),
                        priority: job.priority,
                        seq,
                        not_before: None,
                        lease: Some(LeaseState {
                            lease: *lease,
                            worker: worker.clone(),
                            expires_at: *expires_at,
                        }),
                    },
                );
            }
            JobState::Succeeded
            | JobState::Parked { .. }
            | JobState::Cancelled
            | JobState::Suspended
            | JobState::AwaitingApproval { .. } => {
                inner.index.remove(&job.id);
            }
        }
    }
}

impl EventStore for InMemoryEventStore {
    fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> impl Future<Output = Result<(), EventStoreError>> + Send {
        let result =
            self.locked(|inner| Self::append_locked(inner, stream, expected_version, envelopes));
        async move { result }
    }

    fn load(
        &self,
        stream: JobId,
    ) -> impl Future<Output = Result<Vec<EventEnvelope>, EventStoreError>> + Send {
        let result = self.locked(|inner| {
            inner.streams.get(&stream).cloned().ok_or(EventStoreError::StreamNotFound(stream))
        });
        async move { result }
    }
}
```

- [ ] **Step 5: Wire `lib.rs`**

Replace the body of `crates/koine-store-memory/src/lib.rs` (keep the `//!` line; drop the obsolete `use … as _;` lines):

```rust
//! Koiné in-memory driven adapter for tests: complete port implementations without I/O.

pub mod store;
pub mod test_support;

pub use store::InMemoryEventStore;
pub use test_support::{FixedClock, SeededIds};
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p koine-store-memory && cargo clippy -p koine-store-memory --all-targets -- -D warnings`
Expected: 4 tests PASS; clippy clean.

- [ ] **Step 7: Commit**

```bash
git add crates/koine-store-memory
git commit -m "feat(store-memory): add event store with synchronous dispatch index"
```

---

### Task 10: In-memory dispatcher — the atomic claim

**Files:**
- Create: `crates/koine-store-memory/src/dispatcher.rs`
- Modify: `crates/koine-store-memory/src/lib.rs` (add `pub mod dispatcher;` + `pub use dispatcher::InMemoryDispatcher;`)

**Interfaces:**
- Consumes: Task 9's `InMemoryEventStore` internals (`locked`, `append_locked`, `Inner`, `DispatchEntry`), Task 8's `Dispatcher` trait + `LeasedJob` + `wrap_events`, domain `Job`.
- Produces: `InMemoryDispatcher<G: IdGenerator, C: Clock>::new(store: Arc<InMemoryEventStore>, ids: Arc<G>, clock: Arc<C>)` implementing `Dispatcher` per the ADR 0011 contract.

- [ ] **Step 1: Write the failing tests (inline in `dispatcher.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FixedClock, SeededIds};
    use crate::InMemoryEventStore;
    use chrono::{TimeZone, Utc};
    use koine_application::ports::{Dispatcher as _, EventStore as _, IdGenerator as _};
    use koine_application::wrap_events;
    use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy, WorkerId};
    use std::sync::Arc;
    use std::time::Duration;

    struct Fixture {
        store: Arc<InMemoryEventStore>,
        ids: Arc<SeededIds>,
        clock: Arc<FixedClock>,
        dispatcher: InMemoryDispatcher<SeededIds, FixedClock>,
        queue: QueueName,
        worker: WorkerId,
    }

    fn fixture() -> Fixture {
        let store = Arc::new(InMemoryEventStore::new());
        let ids = Arc::new(SeededIds::new(9));
        let clock = Arc::new(FixedClock::at(
            Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"),
        ));
        let dispatcher =
            InMemoryDispatcher::new(Arc::clone(&store), Arc::clone(&ids), Arc::clone(&clock));
        Fixture {
            store,
            ids,
            clock,
            dispatcher,
            queue: QueueName::new("default").expect("q"),
            worker: WorkerId::new("w1").expect("w"),
        }
    }

    async fn enqueue(f: &Fixture, priority: i16, not_before_secs: Option<u64>) -> JobId {
        let stream = f.ids.job_id();
        let correlation = f.ids.correlation_id();
        let now = koine_application::ports::Clock::now(f.clock.as_ref());
        let not_before = not_before_secs.map(|s| {
            now + chrono::TimeDelta::seconds(i64::try_from(s).expect("secs"))
        });
        let event = Job::initial_event(
            f.queue.clone(),
            serde_json::json!({"job": stream.to_string()}),
            Priority(priority),
            RetryPolicy::default(),
            not_before,
        );
        let envelopes = wrap_events(
            f.ids.as_ref(),
            f.clock.as_ref(),
            stream,
            0,
            correlation,
            None,
            None,
            vec![event],
        );
        f.store.append(stream, 0, envelopes).await.expect("enqueue");
        stream
    }

    #[tokio::test]
    async fn claims_by_priority_then_fifo() {
        let f = fixture();
        let low_first = enqueue(&f, 0, None).await;
        let high = enqueue(&f, 9, None).await;
        let low_second = enqueue(&f, 0, None).await;

        let ttl = Duration::from_secs(30);
        let first = f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim");
        let second = f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim");
        let third = f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim");
        let fourth = f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim");

        assert_eq!(first.expect("job").job_id, high, "highest priority first");
        assert_eq!(second.expect("job").job_id, low_first, "then FIFO");
        assert_eq!(third.expect("job").job_id, low_second);
        assert!(fourth.is_none(), "queue drained");
    }

    #[tokio::test]
    async fn respects_not_before() {
        let f = fixture();
        enqueue(&f, 0, Some(60)).await;
        let ttl = Duration::from_secs(30);
        assert!(
            f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim").is_none(),
            "scheduled job must not be claimable yet"
        );
        f.clock.advance(Duration::from_secs(61));
        assert!(f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim").is_some());
    }

    #[tokio::test]
    async fn claim_appends_the_leased_event() {
        let f = fixture();
        let job_id = enqueue(&f, 0, None).await;
        let claimed = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .expect("job");
        assert_eq!(claimed.job_id, job_id);
        let stream = f.store.load(job_id).await.expect("load");
        assert_eq!(stream.len(), 2);
        assert_eq!(stream[1].event.kind(), "leased");
        assert_eq!(stream[1].correlation_id, stream[0].correlation_id, "lineage carried");
    }

    #[tokio::test]
    async fn extend_and_expiry() {
        let f = fixture();
        enqueue(&f, 0, None).await;
        let claimed = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .expect("job");

        let now = koine_application::ports::Clock::now(f.clock.as_ref());
        assert!(f.dispatcher.expired(now).await.expect("expired").is_empty());

        f.clock.advance(Duration::from_secs(20));
        assert!(
            f.dispatcher.extend_lease(claimed.lease, Duration::from_secs(30)).await.expect("hb"),
            "live lease extends"
        );

        f.clock.advance(Duration::from_secs(31));
        let now = koine_application::ports::Clock::now(f.clock.as_ref());
        assert_eq!(f.dispatcher.expired(now).await.expect("expired"), vec![claimed.job_id]);
        assert!(
            !f.dispatcher.extend_lease(claimed.lease, Duration::from_secs(30)).await.expect("hb"),
            "expired lease refuses extension"
        );
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-store-memory dispatcher`
Expected: compile error (`dispatcher` module missing).

- [ ] **Step 3: Implement `dispatcher.rs`**

```rust
//! In-memory `Dispatcher`: the claim-and-lease composite of ADR 0011-b,
//! atomic under the store's single mutex exactly as the Postgres adapter
//! will be atomic under one transaction.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use koine_application::ports::{
    Clock, DispatchError, Dispatcher, EventStoreError, IdGenerator, LeasedJob,
};
use koine_application::wrap_events;
use koine_domain::{Job, JobId, LeaseId, QueueName, WorkerId};

use crate::store::{InMemoryEventStore, Inner};

/// Dispatcher over the in-memory store.
pub struct InMemoryDispatcher<G, C> {
    store: Arc<InMemoryEventStore>,
    ids: Arc<G>,
    clock: Arc<C>,
}

impl<G: IdGenerator, C: Clock> InMemoryDispatcher<G, C> {
    /// New dispatcher sharing the store's state.
    #[must_use]
    pub fn new(store: Arc<InMemoryEventStore>, ids: Arc<G>, clock: Arc<C>) -> Self {
        Self { store, ids, clock }
    }

    fn pick_eligible(
        inner: &Inner,
        queue: &QueueName,
        now: DateTime<Utc>,
    ) -> Option<JobId> {
        inner
            .index
            .iter()
            .filter(|(_, entry)| {
                entry.queue == *queue
                    && entry.lease.is_none()
                    && entry.not_before.is_none_or(|t| t <= now)
            })
            .max_by_key(|(_, entry)| (entry.priority, std::cmp::Reverse(entry.seq)))
            .map(|(job_id, _)| *job_id)
    }

    fn claim(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeasedJob>, DispatchError> {
        let now = self.clock.now();
        self.store
            .locked(|inner| {
                let Some(job_id) = Self::pick_eligible(inner, queue, now) else {
                    return Ok(None);
                };
                let stream = inner
                    .streams
                    .get(&job_id)
                    .cloned()
                    .ok_or(EventStoreError::StreamNotFound(job_id))?;
                let job = Job::from_events(&stream)
                    .map_err(|e| EventStoreError::Backend(format!("fold: {e}")))?;
                let lease = self.ids.lease_id();
                let event = job
                    .lease(worker.clone(), lease, now, ttl)
                    .map_err(|e| EventStoreError::Backend(format!("index/state drift: {e}")))?;
                let correlation_id = stream[0].correlation_id;
                let traceparent = stream[0].traceparent.clone();
                let causation_id = stream.last().map(|env| env.event_id);
                let envelopes = wrap_events(
                    self.ids.as_ref(),
                    self.clock.as_ref(),
                    job_id,
                    job.version,
                    correlation_id,
                    causation_id,
                    traceparent.clone(),
                    vec![event],
                );
                let expires_at = match &envelopes[0].event {
                    koine_domain::JobEvent::Leased { expires_at, .. } => *expires_at,
                    _ => return Err(EventStoreError::Backend("lease produced non-lease".into())),
                };
                InMemoryEventStore::append_locked(inner, job_id, job.version, envelopes)?;
                Ok(Some(LeasedJob {
                    job_id,
                    queue: job.queue,
                    payload: job.payload,
                    attempt: job.attempt,
                    lease,
                    expires_at,
                    correlation_id,
                    traceparent,
                }))
            })
            .map_err(DispatchError::from)
    }
}

impl<G: IdGenerator, C: Clock> Dispatcher for InMemoryDispatcher<G, C> {
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send {
        let result = self.claim(queue, worker, ttl);
        async move { result }
    }

    fn extend_lease(
        &self,
        lease: LeaseId,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool, DispatchError>> + Send {
        let now = self.clock.now();
        let deadline = now + chrono::TimeDelta::from_std(ttl).unwrap_or(chrono::TimeDelta::MAX);
        let result = self
            .store
            .locked(|inner| {
                for entry in inner.index.values_mut() {
                    if let Some(state) = entry.lease.as_mut() {
                        if state.lease == lease {
                            if state.expires_at <= now {
                                return Ok(false);
                            }
                            state.expires_at = deadline;
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            })
            .map_err(DispatchError::from);
        async move { result }
    }

    fn expired(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<JobId>, DispatchError>> + Send {
        let result = self
            .store
            .locked(|inner| {
                let mut ids: Vec<JobId> = inner
                    .index
                    .iter()
                    .filter(|(_, entry)| {
                        entry.lease.as_ref().is_some_and(|l| l.expires_at <= now)
                    })
                    .map(|(id, _)| *id)
                    .collect();
                ids.sort();
                Ok(ids)
            })
            .map_err(DispatchError::from);
        async move { result }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p koine-store-memory && cargo clippy -p koine-store-memory --all-targets -- -D warnings`
Expected: 8 tests PASS; clippy clean. (Note: `Priority` derives `Ord`, so `max_by_key` on `(priority, Reverse(seq))` is the exact claim order.)

- [ ] **Step 5: Commit**

```bash
git add crates/koine-store-memory
git commit -m "feat(store-memory): add atomic claim dispatcher"
```

---

### Task 11: Use cases — enqueue, worker acks, cancel

**Files:**
- Create: `crates/koine-application/src/use_cases/mod.rs`, `crates/koine-application/src/use_cases/enqueue.rs`, `crates/koine-application/src/use_cases/worker_ack.rs`, `crates/koine-application/src/use_cases/cancel.rs`
- Create: `crates/koine-store-memory/tests/lifecycle.rs` (first half)
- Modify: `crates/koine-application/src/lib.rs`

**Interfaces:**
- Consumes: ports + `wrap_events` + `Lineage` (Task 8), domain commands (Task 6), memory store (Tasks 9–10, in tests).
- Produces: `EnqueueJob { store, ids, clock }` with `execute(EnqueueCommand { queue, payload, priority, retry_policy, not_before, lineage }) -> Result<JobId, EventStoreError>`; `WorkerAck { store, ids, clock }` with `start(job_id, &WorkerId) -> Result<(), AckError>`, `succeed(job_id, &WorkerId, LeaseId, Option<Value>) -> Result<AckOutcome, AckError>`, `fail(job_id, &WorkerId, LeaseId, JobError) -> Result<AckOutcome, AckError>`; `AckOutcome::{Recorded, Conflict}`; `AckError::{Store(EventStoreError), Domain(DomainError)}`; `CancelJob { store, ids, clock }` with `execute(job_id, Option<String>) -> Result<(), AckError>`.

- [ ] **Step 1: Write the failing ring-2 tests (`crates/koine-store-memory/tests/lifecycle.rs`)**

```rust
//! Ring-2 lifecycle tests: use cases against the complete in-memory
//! adapters (testing-policy ring 2 — fast, no Docker).

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use koine_application::use_cases::cancel::CancelJob;
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::worker_ack::{AckOutcome, WorkerAck};
use koine_application::{Lineage, ports::Dispatcher as _, ports::EventStore as _};
use koine_domain::{JobError, JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, InMemoryDispatcher, InMemoryEventStore, SeededIds};

struct World {
    store: Arc<InMemoryEventStore>,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    dispatcher: InMemoryDispatcher<SeededIds, FixedClock>,
    queue: QueueName,
    worker: WorkerId,
}

fn world() -> World {
    let store = Arc::new(InMemoryEventStore::new());
    let ids = Arc::new(SeededIds::new(11));
    let clock = Arc::new(FixedClock::at(
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"),
    ));
    let dispatcher =
        InMemoryDispatcher::new(Arc::clone(&store), Arc::clone(&ids), Arc::clone(&clock));
    World {
        store,
        ids,
        clock,
        dispatcher,
        queue: QueueName::new("default").expect("q"),
        worker: WorkerId::new("w1").expect("w"),
    }
}

fn tight_policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(2),
    }
}

async fn enqueue(w: &World, policy: RetryPolicy) -> JobId {
    EnqueueJob { store: w.store.as_ref(), ids: w.ids.as_ref(), clock: w.clock.as_ref() }
        .execute(EnqueueCommand {
            queue: w.queue.clone(),
            payload: serde_json::json!({"work": true}),
            priority: Priority(0),
            retry_policy: policy,
            not_before: None,
            lineage: Lineage::default(),
        })
        .await
        .expect("enqueue")
}

async fn kinds(w: &World, job: JobId) -> Vec<&'static str> {
    w.store.load(job).await.expect("load").iter().map(|env| env.event.kind()).collect()
}

fn ack<'a>(w: &'a World) -> WorkerAck<'a, InMemoryEventStore, SeededIds, FixedClock> {
    WorkerAck { store: w.store.as_ref(), ids: w.ids.as_ref(), clock: w.clock.as_ref() }
}

#[tokio::test]
async fn happy_path_records_the_full_story() {
    let w = world();
    let job = enqueue(&w, RetryPolicy::default()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    assert_eq!(leased.job_id, job);
    assert_eq!(leased.attempt, 0);

    ack(&w).start(job, &w.worker).await.expect("start");
    let outcome = ack(&w)
        .succeed(job, &w.worker, leased.lease, Some(serde_json::json!("done")))
        .await
        .expect("succeed");
    assert_eq!(outcome, AckOutcome::Recorded);
    assert_eq!(kinds(&w, job).await, vec!["enqueued", "leased", "started", "succeeded"]);
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none(),
        "terminal job must leave the dispatch index"
    );
}

#[tokio::test]
async fn retryable_failure_backs_off_then_retries() {
    let w = world();
    let job = enqueue(&w, tight_policy()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    ack(&w).start(job, &w.worker).await.expect("start");
    let outcome = ack(&w)
        .fail(
            job,
            &w.worker,
            leased.lease,
            JobError { kind: "io".into(), message: "boom".into(), stacktrace: None, retryable: true },
        )
        .await
        .expect("fail");
    assert_eq!(outcome, AckOutcome::Recorded);
    assert_eq!(
        kinds(&w, job).await,
        vec!["enqueued", "leased", "started", "failed", "retry_scheduled"]
    );
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none(),
        "backoff must gate the retry"
    );
    w.clock.advance(Duration::from_secs(3));
    let retried = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("retry after backoff");
    assert_eq!(retried.attempt, 1);
}

#[tokio::test]
async fn non_retryable_failure_parks_immediately() {
    let w = world();
    let job = enqueue(&w, RetryPolicy::default()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    ack(&w).start(job, &w.worker).await.expect("start");
    ack(&w)
        .fail(
            job,
            &w.worker,
            leased.lease,
            JobError { kind: "bug".into(), message: "bad input".into(), stacktrace: None, retryable: false },
        )
        .await
        .expect("fail");
    assert_eq!(kinds(&w, job).await.last(), Some(&"parked"));
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none()
    );
}

#[tokio::test]
async fn cancel_removes_a_pending_job() {
    let w = world();
    let job = enqueue(&w, RetryPolicy::default()).await;
    CancelJob { store: w.store.as_ref(), ids: w.ids.as_ref(), clock: w.clock.as_ref() }
        .execute(job, Some("operator".into()))
        .await
        .expect("cancel");
    assert_eq!(kinds(&w, job).await, vec!["enqueued", "cancelled"]);
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none()
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-store-memory --test lifecycle`
Expected: compile error (`use_cases` module missing).

- [ ] **Step 3: Implement the use cases**

`crates/koine-application/src/use_cases/mod.rs`:

```rust
//! Application use cases: thin orchestration over domain commands and ports.

pub mod cancel;
pub mod enqueue;
pub mod worker_ack;
```

`crates/koine-application/src/use_cases/enqueue.rs`:

```rust
//! Accepting new jobs into the log.

use chrono::{DateTime, Utc};
use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy};
use serde_json::Value;

use crate::lineage::{wrap_events, Lineage};
use crate::ports::{Clock, EventStore, EventStoreError, IdGenerator};

/// Command input for [`EnqueueJob`].
#[derive(Debug, Clone)]
pub struct EnqueueCommand {
    /// Destination queue.
    pub queue: QueueName,
    /// Opaque worker payload.
    pub payload: Value,
    /// Dispatch priority.
    pub priority: Priority,
    /// Retry policy for this job.
    pub retry_policy: RetryPolicy,
    /// Earliest dispatch time.
    pub not_before: Option<DateTime<Utc>>,
    /// Caller lineage.
    pub lineage: Lineage,
}

/// Use case: accept a new job.
pub struct EnqueueJob<'a, S, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, G: IdGenerator, C: Clock> EnqueueJob<'_, S, G, C> {
    /// Opens a new stream with `enqueued` (version 1) and returns the job id.
    pub async fn execute(&self, cmd: EnqueueCommand) -> Result<JobId, EventStoreError> {
        let job_id = self.ids.job_id();
        let correlation =
            cmd.lineage.correlation_id.unwrap_or_else(|| self.ids.correlation_id());
        let event = Job::initial_event(
            cmd.queue,
            cmd.payload,
            cmd.priority,
            cmd.retry_policy,
            cmd.not_before,
        );
        let envelopes = wrap_events(
            self.ids,
            self.clock,
            job_id,
            0,
            correlation,
            cmd.lineage.causation_id,
            cmd.lineage.traceparent,
            vec![event],
        );
        self.store.append(job_id, 0, envelopes).await?;
        Ok(job_id)
    }
}
```

`crates/koine-application/src/use_cases/worker_ack.rs`:

```rust
//! Worker-facing acks: start, succeed, fail. A stale ack (lease no longer
//! held) is never dropped — it becomes a `late_ack_conflict` record
//! (spec §3: information is never lost).

use koine_domain::{
    CorrelationId, DomainError, EventEnvelope, EventId, Job, JobError, JobId, LeaseId,
    ReportedOutcome, WorkerId,
};
use serde_json::Value;
use thiserror::Error;

use crate::lineage::wrap_events;
use crate::ports::{Clock, EventStore, EventStoreError, IdGenerator};

/// Errors from worker acks.
#[derive(Debug, Error)]
pub enum AckError {
    /// Store failure.
    #[error(transparent)]
    Store(#[from] EventStoreError),
    /// Domain rejection that is not a stale-lease situation.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// How an ack was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    /// Recorded as the worker intended.
    Recorded,
    /// The lease was no longer held — recorded as a conflict.
    Conflict,
}

/// Use case: worker acks.
pub struct WorkerAck<'a, S, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, G: IdGenerator, C: Clock> WorkerAck<'_, S, G, C> {
    /// Worker signals execution started.
    pub async fn start(&self, job_id: JobId, worker: &WorkerId) -> Result<(), AckError> {
        let (job, stream) = self.load(job_id).await?;
        let event = job.start(worker)?;
        self.append(&job, &stream, vec![event]).await?;
        Ok(())
    }

    /// Worker reports success.
    pub async fn succeed(
        &self,
        job_id: JobId,
        worker: &WorkerId,
        lease: LeaseId,
        result: Option<Value>,
    ) -> Result<AckOutcome, AckError> {
        let (job, stream) = self.load(job_id).await?;
        match job.succeed(lease, result) {
            Ok(event) => {
                self.append(&job, &stream, vec![event]).await?;
                Ok(AckOutcome::Recorded)
            }
            Err(_) => {
                self.record_conflict(&job, &stream, worker, lease, ReportedOutcome::Succeeded)
                    .await?;
                Ok(AckOutcome::Conflict)
            }
        }
    }

    /// Worker reports failure; the retry decision rides the same append.
    pub async fn fail(
        &self,
        job_id: JobId,
        worker: &WorkerId,
        lease: LeaseId,
        error: JobError,
    ) -> Result<AckOutcome, AckError> {
        let (job, stream) = self.load(job_id).await?;
        match job.fail(lease, error, self.clock.now(), self.ids.jitter_seed()) {
            Ok(events) => {
                self.append(&job, &stream, events).await?;
                Ok(AckOutcome::Recorded)
            }
            Err(_) => {
                self.record_conflict(&job, &stream, worker, lease, ReportedOutcome::Failed)
                    .await?;
                Ok(AckOutcome::Conflict)
            }
        }
    }

    async fn load(&self, job_id: JobId) -> Result<(Job, Vec<EventEnvelope>), AckError> {
        let stream = self.store.load(job_id).await?;
        let job = Job::from_events(&stream)?;
        Ok((job, stream))
    }

    async fn append(
        &self,
        job: &Job,
        stream: &[EventEnvelope],
        events: Vec<koine_domain::JobEvent>,
    ) -> Result<(), EventStoreError> {
        let (correlation, causation, traceparent) = lineage_of(stream);
        let envelopes = wrap_events(
            self.ids,
            self.clock,
            job.id,
            job.version,
            correlation,
            causation,
            traceparent,
            events,
        );
        self.store.append(job.id, job.version, envelopes).await
    }

    async fn record_conflict(
        &self,
        job: &Job,
        stream: &[EventEnvelope],
        worker: &WorkerId,
        lease: LeaseId,
        reported: ReportedOutcome,
    ) -> Result<(), EventStoreError> {
        let event = Job::late_ack(worker.clone(), lease, reported);
        self.append(job, stream, vec![event]).await
    }
}

fn lineage_of(stream: &[EventEnvelope]) -> (CorrelationId, Option<EventId>, Option<String>) {
    let correlation = stream.first().map_or_else(
        || CorrelationId::new(uuid::Uuid::nil()),
        |env| env.correlation_id,
    );
    let causation = stream.last().map(|env| env.event_id);
    let traceparent = stream.first().and_then(|env| env.traceparent.clone());
    (correlation, causation, traceparent)
}
```

`crates/koine-application/src/use_cases/cancel.rs`:

```rust
//! Operator/agent cancellation.

use koine_domain::{Job, JobId};

use crate::lineage::wrap_events;
use crate::ports::{Clock, EventStore, IdGenerator};
use crate::use_cases::worker_ack::AckError;

/// Use case: cancel a job in any non-terminal state.
pub struct CancelJob<'a, S, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, G: IdGenerator, C: Clock> CancelJob<'_, S, G, C> {
    /// Appends `cancelled` (with optional reason).
    pub async fn execute(&self, job_id: JobId, reason: Option<String>) -> Result<(), AckError> {
        let stream = self.store.load(job_id).await?;
        let job = Job::from_events(&stream)?;
        let event = job.cancel(reason)?;
        let correlation = stream.first().map_or_else(
            || koine_domain::CorrelationId::new(uuid::Uuid::nil()),
            |env| env.correlation_id,
        );
        let causation = stream.last().map(|env| env.event_id);
        let traceparent = stream.first().and_then(|env| env.traceparent.clone());
        let envelopes = wrap_events(
            self.ids,
            self.clock,
            job.id,
            job.version,
            correlation,
            causation,
            traceparent,
            vec![event],
        );
        self.store.append(job.id, job.version, envelopes).await?;
        Ok(())
    }
}
```

Add to `crates/koine-application/src/lib.rs`: `pub mod use_cases;` (keep existing exports).
Also add `uuid = { version = "1", features = ["v7", "serde"] }` is already a dependency (Task 1) — `uuid::Uuid::nil()` needs no feature.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p koine-store-memory --test lifecycle && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 4 tests PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/koine-application crates/koine-store-memory
git commit -m "feat(application): add enqueue, worker-ack, and cancel use cases"
```

---

### Task 12: Use cases — lease, heartbeat, sweep + crash-recovery scenarios

**Files:**
- Create: `crates/koine-application/src/use_cases/lease.rs`, `crates/koine-application/src/use_cases/heartbeat.rs`, `crates/koine-application/src/use_cases/sweep.rs`
- Modify: `crates/koine-application/src/use_cases/mod.rs`, `crates/koine-store-memory/tests/lifecycle.rs` (append)

**Interfaces:**
- Consumes: everything above.
- Produces: `LeaseNextJob { dispatcher }` with `execute(&QueueName, &WorkerId, Duration) -> Result<Option<LeasedJob>, DispatchError>`; `Heartbeat { dispatcher }` with `execute(LeaseId, Duration) -> Result<bool, DispatchError>`; `SweepExpiredLeases { store, dispatcher, ids, clock }` with `execute() -> Result<u32, SweepError>`; `SweepError::{Store(EventStoreError), Dispatch(DispatchError), Domain(DomainError)}`.

- [ ] **Step 1: Write the failing tests (append to `lifecycle.rs`)**

```rust
use koine_application::use_cases::heartbeat::Heartbeat;
use koine_application::use_cases::sweep::SweepExpiredLeases;

fn sweeper<'a>(
    w: &'a World,
) -> SweepExpiredLeases<'a, InMemoryEventStore, InMemoryDispatcher<SeededIds, FixedClock>, SeededIds, FixedClock>
{
    SweepExpiredLeases {
        store: w.store.as_ref(),
        dispatcher: &w.dispatcher,
        ids: w.ids.as_ref(),
        clock: w.clock.as_ref(),
    }
}

#[tokio::test]
async fn worker_crash_is_recovered_by_the_sweep() {
    let w = world();
    let job = enqueue(&w, tight_policy()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    // the worker "dies" here: no start, no ack, no heartbeat
    w.clock.advance(Duration::from_secs(31));
    assert_eq!(sweeper(&w).execute().await.expect("sweep"), 1);
    let story = kinds(&w, job).await;
    assert_eq!(story, vec!["enqueued", "leased", "lease_expired", "retry_scheduled"]);

    w.clock.advance(Duration::from_secs(3));
    let retried = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("recovered");
    assert_eq!(retried.job_id, job);
    assert_eq!(retried.attempt, 1, "crash counts as an attempt");
    let _ = leased;
}

#[tokio::test]
async fn late_ack_after_expiry_is_recorded_never_lost() {
    let w = world();
    let job = enqueue(&w, tight_policy()).await;
    let stale = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(31));
    sweeper(&w).execute().await.expect("sweep");

    // the presumed-dead worker comes back and acks with its stale lease
    let outcome = ack(&w)
        .succeed(job, &w.worker, stale.lease, None)
        .await
        .expect("late ack path");
    assert_eq!(outcome, AckOutcome::Conflict);
    assert_eq!(kinds(&w, job).await.last(), Some(&"late_ack_conflict"));

    // and the job's real lifecycle is untouched: it retries normally
    w.clock.advance(Duration::from_secs(3));
    let retried = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("still claimable");
    ack(&w).start(job, &w.worker).await.expect("start");
    let outcome =
        ack(&w).succeed(job, &w.worker, retried.lease, None).await.expect("succeed");
    assert_eq!(outcome, AckOutcome::Recorded);
    assert_eq!(kinds(&w, job).await.last(), Some(&"succeeded"));
}

#[tokio::test]
async fn heartbeats_keep_the_lease_alive() {
    let w = world();
    let _job = enqueue(&w, tight_policy()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(20));
    assert!(Heartbeat { dispatcher: &w.dispatcher }
        .execute(leased.lease, Duration::from_secs(30))
        .await
        .expect("heartbeat"));
    w.clock.advance(Duration::from_secs(20));
    assert_eq!(sweeper(&w).execute().await.expect("sweep"), 0, "extended lease is alive");
    w.clock.advance(Duration::from_secs(11));
    assert_eq!(sweeper(&w).execute().await.expect("sweep"), 1, "then it expires");
}

#[tokio::test]
async fn repeated_crashes_exhaust_into_parked() {
    let w = world();
    let policy = RetryPolicy { max_attempts: 1, ..tight_policy() };
    let job = enqueue(&w, policy).await;
    w.dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(31));
    sweeper(&w).execute().await.expect("sweep");
    assert_eq!(kinds(&w, job).await.last(), Some(&"parked"));
    w.clock.advance(Duration::from_secs(60));
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none(),
        "parked jobs await repair, not dispatch"
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p koine-store-memory --test lifecycle`
Expected: compile error (`sweep`/`heartbeat` modules missing).

- [ ] **Step 3: Implement the three use cases**

Append to `use_cases/mod.rs`:

```rust
pub mod heartbeat;
pub mod lease;
pub mod sweep;
```

`crates/koine-application/src/use_cases/lease.rs`:

```rust
//! Claiming work (thin over the `Dispatcher` port — the atomicity lives in
//! the adapter, ADR 0011).

use std::time::Duration;

use koine_domain::{QueueName, WorkerId};

use crate::ports::{DispatchError, Dispatcher, LeasedJob};

/// Use case: claim the next eligible job.
pub struct LeaseNextJob<'a, D> {
    /// Dispatcher port.
    pub dispatcher: &'a D,
}

impl<D: Dispatcher> LeaseNextJob<'_, D> {
    /// Claims for `worker` on `queue`, or returns `None` when drained.
    pub async fn execute(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeasedJob>, DispatchError> {
        self.dispatcher.lease_next(queue, worker, ttl).await
    }
}
```

`crates/koine-application/src/use_cases/heartbeat.rs`:

```rust
//! Lease keep-alive. Ephemeral by design: no event is written (ADR 0011-c).

use std::time::Duration;

use koine_domain::LeaseId;

use crate::ports::{DispatchError, Dispatcher};

/// Use case: extend a live lease.
pub struct Heartbeat<'a, D> {
    /// Dispatcher port.
    pub dispatcher: &'a D,
}

impl<D: Dispatcher> Heartbeat<'_, D> {
    /// Returns `false` when the lease is gone — the worker must stop.
    pub async fn execute(&self, lease: LeaseId, ttl: Duration) -> Result<bool, DispatchError> {
        self.dispatcher.extend_lease(lease, ttl).await
    }
}
```

`crates/koine-application/src/use_cases/sweep.rs`:

```rust
//! The sweep: converts expired leases into recorded history
//! (`lease_expired` + retry decision). The broker's heartbeat-side of the
//! crash-recovery guarantee.

use koine_domain::{DomainError, Job};
use thiserror::Error;

use crate::lineage::wrap_events;
use crate::ports::{
    Clock, DispatchError, Dispatcher, EventStore, EventStoreError, IdGenerator,
};

/// Errors from the sweep.
#[derive(Debug, Error)]
pub enum SweepError {
    /// Store failure.
    #[error(transparent)]
    Store(#[from] EventStoreError),
    /// Dispatcher failure.
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
    /// Domain rejection outside the expected race.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// Use case: sweep expired leases.
pub struct SweepExpiredLeases<'a, S, D, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Dispatcher port.
    pub dispatcher: &'a D,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, D: Dispatcher, G: IdGenerator, C: Clock>
    SweepExpiredLeases<'_, S, D, G, C>
{
    /// Expires every overdue lease; returns how many jobs were swept.
    /// Races (a job acked between listing and folding, or a concurrent
    /// append) are skipped — the next sweep sees the truth.
    pub async fn execute(&self) -> Result<u32, SweepError> {
        let now = self.clock.now();
        let mut swept = 0;
        for job_id in self.dispatcher.expired(now).await? {
            let stream = self.store.load(job_id).await?;
            let job = Job::from_events(&stream)?;
            let Ok(events) = job.expire_lease(now, self.ids.jitter_seed()) else {
                continue; // already acked or otherwise moved on — not expired
            };
            let correlation = stream.first().map_or_else(
                || koine_domain::CorrelationId::new(uuid::Uuid::nil()),
                |env| env.correlation_id,
            );
            let causation = stream.last().map(|env| env.event_id);
            let traceparent = stream.first().and_then(|env| env.traceparent.clone());
            let envelopes = wrap_events(
                self.ids,
                self.clock,
                job.id,
                job.version,
                correlation,
                causation,
                traceparent,
                events,
            );
            match self.store.append(job.id, job.version, envelopes).await {
                Ok(()) => swept += 1,
                Err(EventStoreError::VersionConflict { .. }) => {} // lost the race: skip
                Err(other) => return Err(other.into()),
            }
        }
        Ok(swept)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: all workspace tests PASS (ring 1 + ring 2 complete); clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/koine-application crates/koine-store-memory
git commit -m "feat(application): add lease, heartbeat, and sweep use cases"
```

---

### Task 13: Closeout — TLA+ skeleton, wiki pages, epic and backlog records

**Files:**
- Create: `docs/formal/lease_protocol.tla`, `docs/formal/README.md`
- Create: `docs/architecture/koine-domain.md`, `docs/architecture/koine-application.md`, `docs/architecture/koine-store-memory.md`, `docs/architecture/event-model.md`
- Create: `.apptlas/backlog/done/phase-1a-domain-core.md`
- Modify: `docs/architecture/README.md` (page table), `docs/architecture/overview.md` (crate rows now real), `.apptlas/epics/phase-1-event-sourced-core.md` (State line), `CLAUDE.md` (phase log)

**Interfaces:**
- Consumes: everything delivered above.
- Produces: DoD items 4–6 satisfied for 1A; the record 1B's plan builds on.

- [ ] **Step 1: Write the TLA+ skeleton**

`docs/formal/lease_protocol.tla`:

```text
---- MODULE lease_protocol ----
(* DRAFT SKELETON — written alongside phase 1A's state machine so model and
   code co-evolve (phase-2 epic risk mitigation). TLC configuration and the
   checked properties land with phase 2's data plane. *)

EXTENDS Naturals

CONSTANTS Workers, MaxAttempts

VARIABLES state, attempt, holder

vars == <<state, attempt, holder>>

States == {"pending", "leased", "running", "succeeded", "parked", "cancelled"}

NoWorker == "none"

Init == state = "pending" /\ attempt = 0 /\ holder = NoWorker

Lease(w) ==
    /\ state = "pending"
    /\ state' = "leased" /\ holder' = w
    /\ UNCHANGED attempt

Start ==
    /\ state = "leased"
    /\ state' = "running"
    /\ UNCHANGED <<attempt, holder>>

Succeed ==
    /\ state = "running"
    /\ state' = "succeeded" /\ holder' = NoWorker
    /\ UNCHANGED attempt

Fail ==
    /\ state = "running"
    /\ attempt' = attempt + 1
    /\ holder' = NoWorker
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"

Expire ==
    /\ state \in {"leased", "running"}
    /\ attempt' = attempt + 1
    /\ holder' = NoWorker
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"

Cancel ==
    /\ state \in {"pending", "leased", "running", "parked"}
    /\ state' = "cancelled" /\ holder' = NoWorker
    /\ UNCHANGED attempt

Next == Start \/ Succeed \/ Fail \/ Expire \/ Cancel \/ (\E w \in Workers : Lease(w))

TypeOK ==
    /\ state \in States
    /\ attempt \in 0..MaxAttempts
    /\ holder \in Workers \cup {NoWorker}

(* Properties to check with TLC in phase 2 (needs per-lease identity added):
   - NoDualLease: two workers never hold a live lease simultaneously
   - NoLostJob: every job ends succeeded/parked/cancelled or stays reachable
   - LateAckSafety: a stale ack never changes lifecycle state *)
====
```

`docs/formal/README.md`:

```markdown
# Formal models

Draft TLA+ models co-evolving with the implementation. Status: **skeleton** —
`lease_protocol.tla` mirrors `koine-domain`'s `Job` state machine (phase 1A).
TLC model-checking, the per-lease identity needed for the dual-lease and
late-ack properties, and CI integration are phase-2 deliverables
(`.apptlas/epics/phase-2-data-plane.md`, item 1). If TLC later finds a
counterexample in behavior phase 1 already implements, that is a phase-1
fidelity finding (phase-2 epic, risks).
```

- [ ] **Step 2: Write the four wiki pages**

Each page follows the documentation-policy template (What / How / Why / Boundaries). Content requirements (write them concretely from the delivered code — navigable names, ADR links, truthful phase status):

- `koine-domain.md` — What: pure event-sourced core (`Job`, `JobEvent`, `RetryPolicy`). How: state = fold (`Job::from_events`/`apply`); commands validate and emit events; transition table in `job.rs`; deterministic jitter (`retry.rs`, SplitMix64); proptest suite `state_machine_props.rs` guarantees no reachable illegal state. Why: ADR 0004 (event log as truth), ADR 0010 (encoding/identity). Boundaries: depends on nothing internal; serde/uuid/chrono/thiserror only (pure data — ADR 0010); no clocks, no randomness (both are ports).
- `koine-application.md` — What: driven ports (`EventStore`, `Dispatcher`, `Clock`, `IdGenerator`) and use cases (`enqueue`, `worker_ack`, `cancel`, `lease`, `heartbeat`, `sweep`). How: native async-fn-in-trait returning `impl Future + Send`; static dispatch; `wrap_events` builds sequential envelopes; late acks become `late_ack_conflict` records. Why: ADR 0006/0011 (composite-operation contracts live in adapters; use cases stay thin). Boundaries: → domain only; `OutboxRelay`/`ProjectionStore` ports arrive with 1B (planned — phase 1B).
- `koine-store-memory.md` — What: complete in-memory port implementations proving port neutrality + hosting ring-2 tests. How: one mutex = the "transaction"; `append_locked` re-derives the dispatch entry from folded state (rebuildable projection); `InMemoryDispatcher` claims under the same lock; `FixedClock`/`SeededIds` test doubles. Why: ADR 0005 (in-memory keeps ports honest), ADR 0011. Boundaries: → application + domain; test-oriented, never a production store.
- `event-model.md` — the taxonomy reference: table of all 19 kinds (kind string, v1-active vs reserved-phase-5, emitted by whom), the state diagram from spec §3, envelope fields, lineage rules (correlation carried from `enqueued`, causation = previous event), additive-evolution rules (ADR 0010).

Update `docs/architecture/README.md` page table: add the four new pages with status `Current (phase 1A)`; update `overview.md` crate table rows for `koine-domain`, `koine-application`, `koine-store-memory` from "(phase 1)" to real one-liners (e.g. `koine-domain` → "Aggregates, events, state machines. No async, no I/O — see [koine-domain.md](koine-domain.md)").

- [ ] **Step 3: Record the backlog item, epic state, and phase log**

`.apptlas/backlog/done/phase-1a-domain-core.md` (item-template shape):

```markdown
# Phase 1A — event-sourced domain core (rings 1–2)

- **State:** done
- **Origin:** plan docs/superpowers/plans/2026-07-18-koine-phase-1a-domain-core.md
- **Epic:** ../epics/phase-1-event-sourced-core.md (items 1–7, 12-partial)

## Traceability

- **Implements:** design spec §3 (event model, state machine, delivery
  semantics at the domain/application level); ADRs 0004, 0006, 0010, 0011.

## Acceptance criteria

- [x] AC1: full v1 event taxonomy + reserved kinds, serde-stable — *verify:*
  `cargo test -p koine-domain events` (tag/kind drift test)
- [x] AC2: no event sequence reaches an illegal state — *verify:*
  `cargo test -p koine-domain --test state_machine_props`
- [x] AC3: enqueue→lease→ack/fail→retry→park via use cases against complete
  in-memory adapters — *verify:* `cargo test -p koine-store-memory --test lifecycle`
- [x] AC4: crash recovery (lease expiry → sweep → retry), late-ack conflict
  recording, heartbeat keep-alive — *verify:* lifecycle tests
  `worker_crash_is_recovered_by_the_sweep`,
  `late_ack_after_expiry_is_recorded_never_lost`,
  `heartbeats_keep_the_lease_alive`
- [x] AC5: dispatch index maintained atomically with append (ADR 0006
  contract in memory) — *verify:* `cargo test -p koine-store-memory` store
  tests

## Dependencies

- none (1B: Postgres adapters, outbox, ring 3 — separate plan)

## Evidence (filled at close)

<!-- test summary + make ci output + review verdicts, filled at execution -->

## Spec-fidelity statement (filled at close)

Faithful to spec §3 at rings 1–2, with recorded dispositions:
- Spec §2 names a `LeaseManager` port; delivered as `Dispatcher`
  (claim/extend/expired composite) — semantics defined by ADR 0011
  (disposition: ADR).
- Epic item 5 asks for ports "generic over aggregate/event"; delivered
  concrete to `EventEnvelope`/`JobEvent` — a conscious YAGNI while `Job` is
  the only aggregate. The kineticrs lesson is honored by its actual failure
  mode instead: no adapter binds to event-variant internals (the memory
  store projects from folded state, not from matching variants beyond the
  fold), so generifying later is additive (disposition: recorded here;
  revisit when a second aggregate exists).
- `OutboxRelay`/`ProjectionStore` ports and the Postgres adapter are 1B
  (disposition: split plan, epic items 8–11).
- Heartbeat extensions are ephemeral (no event) — per spec §3 itself and
  ADR 0011-c.
```

Epic `.apptlas/epics/phase-1-event-sourced-core.md`: change `- **State:** next up` to `- **State:** ongoing — 1A (domain core, rings 1–2) delivered; 1B (Postgres, outbox, ring 3) next`.

`CLAUDE.md` phase log, append: `- 2026-07-XX — Phase 1A complete: event-sourced domain core green on rings 1–2. Next: phase 1B plan (Postgres store, outbox, dispatch projection, ring 3).`

- [ ] **Step 4: Full gate and commit**

Run: `make ci`
Expected: `✓ all CI checks green` (markdownlint covers the new pages; typos covers the .tla file).

```bash
git add docs/formal docs/architecture .apptlas CLAUDE.md
git commit -m "docs: close out phase 1a with wiki, formal skeleton, and records"
```

---

## Not in this plan (deliberately — 1B and beyond)

- **Postgres adapter, migrations, outbox relay, `OutboxRelay`/`ProjectionStore` ports, ring-3 testcontainers suite, CI Docker strategy** — plan 1B, written after 1A execution.
- **`koine-server` dev-loop command** (epic item 11) — 1B, where a real store makes it meaningful.
- **Kani pilot** (epic item 13, stretch) — evaluated after 1B.
- **TLC model checking + protocol properties** — phase 2 (the skeleton ships now so model and code co-evolve).
