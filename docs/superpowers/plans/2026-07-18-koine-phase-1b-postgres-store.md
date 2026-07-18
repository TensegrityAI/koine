# Koiné Phase 1B — Postgres Store Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the production store — Postgres event store with optimistic concurrency, dispatch projection updated in the append transaction, `SELECT … FOR UPDATE SKIP LOCKED` dispatcher, transactional outbox with a claim-delete relay — proven by ring-3 testcontainers tests against the real migrations, plus the 1A hardening backlog item and the `koine-server dev-loop` product exercise.

**Architecture:** `koine-store-postgres` implements the same ports the memory store proved, honoring identical contracts (ADR 0006/0011) with one transaction where memory used one mutex hold. The dispatch row is re-derived from the folded aggregate inside the append transaction (same rebuildable-projection semantics as 1A). Outbox rows ride the append transaction; a relay claims batches via SKIP LOCKED and deletes after the sink acks (ADR 0012 — gap-safe without position tracking). Hardening lands first so Postgres is built against final APIs.

**Tech Stack:** sqlx 0.8 (runtime-tokio, postgres, runtime queries — no macros, ADR 0012), testcontainers (ring 3), tokio, chrono/uuid.

**Reference:** epic `.apptlas/epics/phase-1-event-sourced-core.md` items 8–11 + 13-stretch-excluded; hardening item `.apptlas/backlog/todo/retry-policy-ttl-bounds-hardening.md`; spec §3; ADRs 0004–0006, 0010–0011 (+0012 from Task 1). 1A delivered: domain (19-kind taxonomy), ports (`EventStore`, `Dispatcher`, `Clock`, `IdGenerator`), `lineage_of`/`wrap_events`, memory adapters, 54 tests.

## Global Constraints

- Everything from the 1A plan still binds: strict lints (clippy pedantic, `-D warnings`, `missing_docs`), no `unwrap()`/`expect()` outside tests (integration-test files start with `#![allow(clippy::expect_used)]` + rationale comment — 1A convention), TDD, Conventional Commits ≤72, `make ci` green per commit, event log append-only, kind strings immutable.
- Dependency direction: store-postgres → application + domain (edges exist from phase 0); server → everything. `koine-store-postgres` dev-depends on `koine-store-memory` ONLY for `FixedClock`/`SeededIds` (test doubles) — no production edge.
- **Migrations are append-only**: one numbered file per change, never edit a committed migration. Ring 3 runs the real files via `sqlx::migrate!` — never an inline schema copy (testing-policy).
- sqlx **runtime queries only** (`sqlx::query`/`query_as` — no `query!` macros): no `DATABASE_URL` at build time, no offline cache to maintain; correctness is ring 3's job (ADR 0012).
- Ring-3 tests require Docker (present locally and on ubuntu-latest CI runners). They live in `crates/koine-store-postgres/tests/` and run in the normal `cargo test --workspace` gate.
- The store adapters must pass the SAME behavioral contracts the memory store passes (failures fully side-effect-free — the 1A M1 lesson is a binding contract here: a rolled-back transaction leaves nothing).
- All timestamps `TIMESTAMPTZ`; ids `UUID`; payloads `JSONB` with the serde-internally-tagged encoding from ADR 0010.

## File map

| File | Responsibility |
| --- | --- |
| `docs/adr/0012-postgres-store-mechanics.md` | Schema, runtime queries, claim-delete outbox decisions |
| `crates/koine-store-postgres/migrations/0001_event_store.sql` | Schema: `event_store.{events,dispatch_queue,outbox}` |
| `crates/koine-store-postgres/src/lib.rs` | Wiring + `connect_pool` |
| `crates/koine-store-postgres/src/rows.rs` | Row ↔ `EventEnvelope` mapping |
| `crates/koine-store-postgres/src/store.rs` | `PostgresEventStore` (append+projection+outbox in one tx, load) |
| `crates/koine-store-postgres/src/dispatcher.rs` | `PostgresDispatcher` (SKIP LOCKED claim, extend, expired) |
| `crates/koine-store-postgres/src/relay.rs` | `PostgresOutboxRelay` (claim-delete batches) |
| `crates/koine-store-postgres/tests/support/mod.rs` | Testcontainers harness (`pg()` → container+migrated pool) |
| `crates/koine-store-postgres/tests/{store,dispatcher,outbox,lifecycle,replay}.rs` | Ring-3 suites |
| `crates/koine-application/src/ports.rs` | + `EventSink` trait, `SinkError`, `RelayError` |
| `crates/koine-application/src/use_cases/enqueue.rs` | + `EnqueueError`, policy bounds validation |
| `crates/koine-application/src/use_cases/sweep.rs` | Skip only `IllegalTransition` |
| `crates/koine-domain/src/retry.rs` | + cross-attempt jitter test |
| `crates/koine-store-memory/src/dispatcher.rs` | extend_lease TTL symmetry |
| `crates/koine-server/src/{main,runtime,dev_loop}.rs` | `SystemClock`/`UuidV7Ids`, `dev-loop` subcommand |
| `compose.yaml`, `.env.example` | Local dev Postgres |
| `docs/architecture/{koine-store-postgres,koine-server}.md` + records | Closeout |

---

### Task 1: ADR 0012, dependencies, migrations, and the ring-3 harness

**Files:**
- Create: `docs/adr/0012-postgres-store-mechanics.md`; add row to `docs/adr/INDEX.md`
- Create: `crates/koine-store-postgres/migrations/0001_event_store.sql`
- Create: `crates/koine-store-postgres/tests/support/mod.rs`, `crates/koine-store-postgres/tests/store.rs` (harness smoke test only)
- Create: `compose.yaml`, `.env.example`
- Modify: `crates/koine-store-postgres/Cargo.toml`, `crates/koine-server/Cargo.toml`, `crates/koine-store-postgres/src/lib.rs`

**Interfaces:**
- Consumes: phase-0 crate stubs; 1A ports.
- Produces: `koine_store_postgres::connect_pool(url: &str) -> Result<sqlx::PgPool, sqlx::Error>` (runs migrations); `MIGRATOR: sqlx::migrate::Migrator`; test harness `support::pg() -> (ContainerAsync<Postgres>, PgPool)`; the schema every later task queries.

- [ ] **Step 1: Write ADR 0012**

```markdown
# 0012 — Postgres store mechanics

- **Status:** accepted
- **Date:** 2026-07-18
- **Context:** Phase 1B implements the production adapters for the ports the
  memory store proved (ADR 0005/0006/0011). Three mechanics need fixing:
  schema shape, query style, and how the outbox relay avoids the classic
  sequence-gap hazard.
- **Decision:**
  - **Schema** (`event_store` schema): `events` (append-only; `global_seq`
    identity for total order; `UNIQUE (stream_id, version)` is the
    optimistic-concurrency guard; envelope decomposed into indexed columns +
    `payload JSONB` holding the serde-tagged event); `dispatch_queue` (one
    row per dispatchable/leased job; partial index on
    `(queue, priority DESC, seq)` `WHERE lease_id IS NULL` serves the claim;
    `seq` from a dedicated sequence, assigned once, preserved on updates);
    `outbox` (`outbox_seq` identity; full envelope JSONB for cheap relay
    delivery).
  - **Append** = one transaction: max-version check (explicit
    `SELECT max(version)`; races resolved by the unique constraint mapping
    Postgres error 23505 → `VersionConflict`), event inserts, dispatch row
    re-derived from the FOLDED aggregate (same rebuildable-projection
    contract the memory store honors), outbox inserts. A failed transaction
    leaves nothing — the 1A "failures are side-effect-free" contract.
  - **Claim** = one transaction: `SELECT … FOR UPDATE SKIP LOCKED` on the
    dispatch partial index, fold, domain `lease()`, event+outbox insert,
    dispatch-row update (ADR 0011-b verbatim, tx instead of mutex).
  - **Outbox relay: claim-delete, not positions.** Identity sequences
    interleave under concurrency: a later `outbox_seq` can commit before an
    earlier one, so a position-tracking relay can silently skip rows. The
    relay instead claims a batch `ORDER BY outbox_seq LIMIT n FOR UPDATE
    SKIP LOCKED`, delivers to the sink, and deletes on success (rollback
    re-exposes the rows). Per-stream ordering holds with a single relay
    instance (all 1B needs). Consumer positions arrive with real read
    projections (phase 3) on top of this.
  - **Runtime queries** (`sqlx::query`/`query_as`), not `query!` macros: no
    build-time `DATABASE_URL`, no offline-cache drift; the ring-3 suite
    against real migrations is the correctness gate (testing-policy).
- **Consequences:** append refolds the stream in-tx (correct first;
  benchmark in phase 2 per spec §7 before optimizing); relay concurrency
  deferred; SQL typos surface in ring 3 instead of compile time — accepted
  and covered.
- **Alternatives considered:** log-tailing the events table by `global_seq`
  (gap hazard above); `query!` macros + offline cache (build-time coupling,
  cache churn per schema change); logical replication/CDC (operational
  heavyweight for 1B).
```

INDEX row: `| [0012](0012-postgres-store-mechanics.md) | Postgres store mechanics | accepted | 2026-07-18 |`

- [ ] **Step 2: Write the migration** (`crates/koine-store-postgres/migrations/0001_event_store.sql`)

```sql
CREATE SCHEMA event_store;

CREATE TABLE event_store.events (
    global_seq     BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    stream_id      UUID        NOT NULL,
    version        BIGINT      NOT NULL,
    event_id       UUID        NOT NULL UNIQUE,
    event_type     TEXT        NOT NULL,
    schema_version SMALLINT    NOT NULL,
    payload        JSONB       NOT NULL,
    correlation_id UUID        NOT NULL,
    causation_id   UUID,
    traceparent    TEXT,
    recorded_at    TIMESTAMPTZ NOT NULL,
    CONSTRAINT events_stream_version_unique UNIQUE (stream_id, version)
);

CREATE SEQUENCE event_store.dispatch_seq;

CREATE TABLE event_store.dispatch_queue (
    job_id           UUID PRIMARY KEY,
    queue            TEXT        NOT NULL,
    priority         SMALLINT    NOT NULL,
    seq              BIGINT      NOT NULL DEFAULT nextval('event_store.dispatch_seq'),
    not_before       TIMESTAMPTZ,
    lease_id         UUID,
    worker_id        TEXT,
    lease_expires_at TIMESTAMPTZ
);

CREATE INDEX dispatch_claim_idx
    ON event_store.dispatch_queue (queue, priority DESC, seq)
    WHERE lease_id IS NULL;

CREATE INDEX dispatch_expiry_idx
    ON event_store.dispatch_queue (lease_expires_at)
    WHERE lease_id IS NOT NULL;

CREATE TABLE event_store.outbox (
    outbox_seq BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event_id   UUID  NOT NULL,
    stream_id  UUID  NOT NULL,
    envelope   JSONB NOT NULL
);
```

- [ ] **Step 3: Declare dependencies**

`crates/koine-store-postgres/Cargo.toml` — add under existing internal deps:

```toml
chrono = { version = "0.4", features = ["serde"] }
serde_json = "1"
sqlx = { version = "0.8", default-features = false, features = [
    "runtime-tokio",
    "tls-rustls",
    "postgres",
    "chrono",
    "uuid",
    "json",
    "migrate",
] }
thiserror = "2"
uuid = { version = "1", features = ["v7", "serde"] }

[dev-dependencies]
koine-store-memory = { version = "0.1.0", path = "../koine-store-memory" }
testcontainers = "0.26"
testcontainers-modules = { version = "0.14", features = ["postgres"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

(If cargo reports the testcontainers/modules pair incompatible, use the latest compatible pair and record the exact versions as a deviation.)

`crates/koine-server/Cargo.toml` — add:

```toml
chrono = "0.4"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
uuid = { version = "1", features = ["v7"] }
```

- [ ] **Step 4: `connect_pool` + migrator in `lib.rs`**

Replace `crates/koine-store-postgres/src/lib.rs` body:

```rust
//! Koiné Postgres driven adapter: event store, transactional outbox, projections.

use sqlx::PgPool;

/// Embedded migrations (append-only files under `migrations/`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// Connects and runs migrations. The single entry point composition roots use.
///
/// # Errors
/// Connection or migration failure.
pub async fn connect_pool(url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPool::connect(url).await?;
    MIGRATOR.run(&pool).await.map_err(sqlx::Error::from)?;
    Ok(pool)
}
```

- [ ] **Step 5: Test harness + smoke test**

`crates/koine-store-postgres/tests/support/mod.rs`:

```rust
//! Ring-3 harness: one throwaway Postgres container per test, real migrations.

use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

/// Starts Postgres and returns (container guard, migrated pool). Keep the
/// guard alive for the test's duration or the container stops.
pub async fn pg() -> (ContainerAsync<Postgres>, PgPool) {
    let container = Postgres::default().start().await.expect("start postgres");
    let port = container.get_host_port_ipv4(5432).await.expect("mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = koine_store_postgres::connect_pool(&url).await.expect("connect + migrate");
    (container, pool)
}
```

`crates/koine-store-postgres/tests/store.rs` (starts with the 1A integration-test convention):

```rust
//! Ring-3 contract tests for the Postgres event store.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

#[tokio::test]
async fn migrations_apply_cleanly() {
    let (_guard, pool) = support::pg().await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.events")
        .fetch_one(&pool)
        .await
        .expect("query");
    assert_eq!(n, 0);
}
```

- [ ] **Step 6: `compose.yaml` + `.env.example`**

`compose.yaml`:

```yaml
services:
  postgres:
    image: postgres:17
    environment:
      POSTGRES_PASSWORD: koine
      POSTGRES_USER: koine
      POSTGRES_DB: koine
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
volumes:
  pgdata:
```

`.env.example`:

```bash
# Local development database (compose.yaml)
DATABASE_URL=postgres://koine:koine@localhost:5432/koine
```

- [ ] **Step 7: Verify and commit**

Run: `cargo test -p koine-store-postgres` (pulls the postgres image on first run — may take a minute) then `make ci`
Expected: smoke test PASS; `✓ all CI checks green`.

```bash
git add docs/adr crates/koine-store-postgres crates/koine-server/Cargo.toml Cargo.lock compose.yaml .env.example
git commit -m "feat(store-postgres): add schema, migrations, and ring-3 harness"
```

---

### Task 2: Hardening pack A — domain and memory-store items

**Files:**
- Modify: `crates/koine-domain/src/retry.rs` (test only), `crates/koine-application/src/ports.rs` (doc only), `crates/koine-store-memory/src/dispatcher.rs`, `crates/koine-store-memory/src/store.rs` (test only)

**Interfaces:**
- Consumes: 1A APIs.
- Produces: `extend_lease` now errors on unrepresentable TTL (behavior change consumed by Task 6's Postgres parity); regression tests later tasks rely on as contract.

- [ ] **Step 1: Cross-attempt jitter test** (append to `retry.rs` tests)

```rust
    #[test]
    fn different_attempts_can_differ_for_fixed_seed() {
        let p = RetryPolicy::default();
        let outcomes: std::collections::HashSet<_> =
            (1..16u32).map(|attempt| format!("{:?}", p.decide(attempt, 42))).collect();
        assert!(outcomes.len() > 4, "fixed-seed delays must vary across attempts");
    }
```

- [ ] **Step 2: Seed-entropy doc** — in `ports.rs`, extend `jitter_seed`'s doc to:

```rust
    /// Seed for deterministic retry jitter. Implementations MUST return
    /// high-entropy values (e.g. from the id source); small sequential
    /// counters would correlate delays across jobs (`seed ^ attempt`
    /// collisions).
    fn jitter_seed(&self) -> u64;
```

- [ ] **Step 3: `extend_lease` TTL symmetry** — in `crates/koine-store-memory/src/dispatcher.rs`, replace the `unwrap_or(TimeDelta::MAX)` deadline computation inside `extend_lease` with an error, matching `lease`'s philosophy:

```rust
        let now = self.clock.now();
        let Ok(delta) = chrono::TimeDelta::from_std(ttl) else {
            let result = Err(DispatchError::Backend("ttl out of range".into()));
            return async move { result };
        };
        let deadline = now + delta;
```

(Adjust surrounding code minimally so both arms return the same future type — compute `result` fully before the single trailing `async move { result }`, as the file already does.)

Add test (dispatcher tests):

```rust
    #[tokio::test]
    async fn extend_lease_rejects_unrepresentable_ttl() {
        let f = fixture();
        enqueue(&f, 0, None).await;
        let claimed = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .expect("job");
        let err = f
            .dispatcher
            .extend_lease(claimed.lease, Duration::MAX)
            .await
            .expect_err("must reject");
        assert!(matches!(err, koine_application::DispatchError::Backend(_)));
    }
```

- [ ] **Step 4: Existing-stream fold-reject regression** (append to `store.rs` tests — the final-review probe, now permanent):

```rust
    #[tokio::test]
    async fn fold_rejected_append_on_existing_stream_keeps_prior_events() {
        use koine_application::ports::IdGenerator;
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(6);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store.append(stream, 0, envelopes.clone()).await.expect("enqueue");
        // version-sequential but illegal from Pending: `started` needs a lease
        let bad = koine_application::wrap_events(
            &ids,
            &clock,
            stream,
            1,
            ids.correlation_id(),
            None,
            None,
            vec![koine_domain::JobEvent::Started {
                worker: koine_domain::WorkerId::new("w").expect("w"),
            }],
        );
        let err = store.append(stream, 1, bad).await.expect_err("must not fold");
        assert!(matches!(err, koine_application::EventStoreError::Backend(_)));
        let loaded = store.load(stream).await.expect("prior events survive");
        assert_eq!(loaded, envelopes, "bad batch discarded, stream intact");
    }
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: all green (56+ tests).

```bash
git add crates/koine-domain crates/koine-application crates/koine-store-memory
git commit -m "test: harden jitter, ttl symmetry, and fold-reject contracts"
```

---

### Task 3: Hardening pack B — enqueue bounds validation and sweep error discipline

**Files:**
- Modify: `crates/koine-application/src/use_cases/enqueue.rs`, `crates/koine-application/src/use_cases/sweep.rs`, `crates/koine-store-memory/tests/lifecycle.rs` (new tests + one call-site type change)

**Interfaces:**
- Consumes: 1A use cases.
- Produces: `EnqueueJob::execute` now returns `Result<JobId, EnqueueError>` where `EnqueueError::{InvalidPolicy(&'static str), Store(EventStoreError)}` — Tasks 8/10 use this signature. `SweepExpiredLeases` skips ONLY `IllegalTransition`; other domain errors surface as `SweepError::Domain`.

- [ ] **Step 1: Write the failing tests** (append to `lifecycle.rs`)

```rust
use koine_application::use_cases::enqueue::EnqueueError;

#[tokio::test]
async fn enqueue_rejects_pathological_retry_policies() {
    let w = world();
    let cases = [
        RetryPolicy { max_attempts: 0, ..RetryPolicy::default() },
        RetryPolicy { max_attempts: 20_000, ..RetryPolicy::default() },
        RetryPolicy {
            base_delay: Duration::from_secs(60),
            max_delay: Duration::from_secs(1),
            ..RetryPolicy::default()
        },
        RetryPolicy {
            max_delay: Duration::from_secs(60 * 60 * 24 * 40),
            ..RetryPolicy::default()
        },
    ];
    for policy in cases {
        let err = EnqueueJob { store: w.store.as_ref(), ids: w.ids.as_ref(), clock: w.clock.as_ref() }
            .execute(EnqueueCommand {
                queue: w.queue.clone(),
                payload: serde_json::json!({}),
                priority: Priority(0),
                retry_policy: policy.clone(),
                not_before: None,
                lineage: Lineage::default(),
            })
            .await
            .expect_err("must reject");
        assert!(matches!(err, EnqueueError::InvalidPolicy(_)), "{policy:?}");
    }
}

#[tokio::test]
async fn sweep_surfaces_non_transition_domain_errors() {
    // A poisoned policy that folds fine but overflows chrono at decision time:
    // base/max near u64::MAX ms. Enqueue-side validation now blocks this at
    // the boundary, so construct the stream directly through the store to
    // simulate pre-validation data (or a future migration gap).
    use koine_application::ports::IdGenerator;
    let w = world();
    let stream = w.ids.job_id();
    let poisoned = RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::MAX,
        max_delay: Duration::MAX,
    };
    let event = koine_domain::Job::initial_event(
        w.queue.clone(),
        serde_json::json!({}),
        Priority(0),
        poisoned,
        None,
    );
    let envs = koine_application::wrap_events(
        w.ids.as_ref(),
        w.clock.as_ref(),
        stream,
        0,
        w.ids.correlation_id(),
        None,
        None,
        vec![event],
    );
    w.store.append(stream, 0, envs).await.expect("direct append");
    w.dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(31));
    let err = sweeper(&w).execute().await.expect_err("InvalidTtl must surface");
    assert!(matches!(err, koine_application::use_cases::sweep::SweepError::Domain(_)));
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p koine-store-memory --test lifecycle`
Expected: compile error (`EnqueueError` missing).

- [ ] **Step 3: Implement `EnqueueError` + validation** (in `enqueue.rs`)

```rust
use thiserror::Error;

/// Errors from enqueueing.
#[derive(Debug, Error)]
pub enum EnqueueError {
    /// The retry policy fails the sanity bounds (broker protects itself
    /// from client-supplied pathology — hardening item AC1).
    #[error("invalid retry policy: {0}")]
    InvalidPolicy(&'static str),
    /// Store failure.
    #[error(transparent)]
    Store(#[from] EventStoreError),
}

/// Longest delay any policy may request (30 days).
const MAX_SANE_DELAY: std::time::Duration = std::time::Duration::from_secs(60 * 60 * 24 * 30);
/// Most attempts any policy may request.
const MAX_SANE_ATTEMPTS: u32 = 10_000;

fn validate_policy(policy: &RetryPolicy) -> Result<(), EnqueueError> {
    if policy.max_attempts == 0 {
        return Err(EnqueueError::InvalidPolicy("max_attempts must be >= 1"));
    }
    if policy.max_attempts > MAX_SANE_ATTEMPTS {
        return Err(EnqueueError::InvalidPolicy("max_attempts above sane ceiling"));
    }
    if policy.base_delay > policy.max_delay {
        return Err(EnqueueError::InvalidPolicy("base_delay exceeds max_delay"));
    }
    if policy.max_delay > MAX_SANE_DELAY {
        return Err(EnqueueError::InvalidPolicy("max_delay above 30-day ceiling"));
    }
    Ok(())
}
```

Change `execute`'s signature/first line:

```rust
    pub async fn execute(&self, cmd: EnqueueCommand) -> Result<JobId, EnqueueError> {
        validate_policy(&cmd.retry_policy)?;
```

(Return type change: the trailing `self.store.append(...).await?` now converts via `#[from]`.) Update the `# Errors` doc. Fix the existing `enqueue` helper in `lifecycle.rs` — its `.expect("enqueue")` still compiles (error type changed only).

- [ ] **Step 4: Sweep error discipline** (in `sweep.rs`) — replace the `let Ok(events) = … else { continue }` with:

```rust
            let events = match job.expire_lease(now, self.ids.jitter_seed()) {
                Ok(events) => events,
                // Already acked / state moved on: not expired anymore — skip.
                Err(DomainError::IllegalTransition { .. }) => continue,
                // Anything else (e.g. InvalidTtl from a poisoned policy) is a
                // real fault: surface it, never strand the lease silently.
                Err(other) => return Err(SweepError::Domain(other)),
            };
```

- [ ] **Step 5: Verify and commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: green (both new tests pass; all 1A tests still pass).

```bash
git add crates/koine-application crates/koine-store-memory
git commit -m "feat(application): validate retry-policy bounds and surface sweep faults"
```

---

### Task 4: `PostgresEventStore` — append + projection + outbox in one transaction

**Files:**
- Create: `crates/koine-store-postgres/src/rows.rs`, `crates/koine-store-postgres/src/store.rs`
- Modify: `crates/koine-store-postgres/src/lib.rs`, `crates/koine-store-postgres/tests/store.rs` (append tests)

**Interfaces:**
- Consumes: schema (Task 1), 1A ports/domain.
- Produces: `PostgresEventStore::new(pool: PgPool)` implementing `EventStore`; crate-internal `append_in_tx(tx, stream, expected_version, &[EventEnvelope]) -> Result<Job, EventStoreError>` and `project_in_tx(tx, &Job)` (reused by Task 5's dispatcher and Task 8's rebuild); `rows::envelope_from_row`, `rows::db`.

- [ ] **Step 1: Write the failing contract tests** (append to `tests/store.rs`; add imports at top)

```rust
use koine_application::ports::{EventStore as _, IdGenerator as _};
use koine_application::wrap_events;
use koine_domain::{Job, JobEvent, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::PostgresEventStore;

fn clock() -> FixedClock {
    use chrono::TimeZone as _;
    FixedClock::at(chrono::Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"))
}

fn enqueue_envelopes(
    ids: &SeededIds,
    clock: &FixedClock,
) -> (koine_domain::JobId, Vec<koine_domain::EventEnvelope>) {
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
async fn appends_and_loads_round_trip() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool);
    let ids = SeededIds::new(21);
    let clock = clock();
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store.append(stream, 0, envelopes.clone()).await.expect("append");
    let loaded = store.load(stream).await.expect("load");
    assert_eq!(loaded, envelopes, "column round-trip must be lossless");
}

#[tokio::test]
async fn rejects_version_conflicts() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool);
    let ids = SeededIds::new(22);
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
async fn failed_append_leaves_no_trace_fresh_or_existing() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool);
    let ids = SeededIds::new(23);
    let clock = clock();

    // fresh stream, illegal opener
    let stream = ids.job_id();
    let bad = wrap_events(
        &ids, &clock, stream, 0, ids.correlation_id(), None, None,
        vec![JobEvent::Suspended],
    );
    let err = store.append(stream, 0, bad).await.expect_err("must not fold");
    assert!(matches!(err, koine_application::EventStoreError::Backend(_)));
    assert!(matches!(
        store.load(stream).await.expect_err("no residue"),
        koine_application::EventStoreError::StreamNotFound(_)
    ));

    // existing stream, illegal continuation — prior events survive
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store.append(stream, 0, envelopes.clone()).await.expect("enqueue");
    let bad = wrap_events(
        &ids, &clock, stream, 1, ids.correlation_id(), None, None,
        vec![JobEvent::Started { worker: WorkerId::new("w").expect("w") }],
    );
    store.append(stream, 1, bad).await.expect_err("must not fold");
    assert_eq!(store.load(stream).await.expect("intact"), envelopes);
}

#[tokio::test]
async fn append_maintains_dispatch_row_and_outbox() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool.clone());
    let ids = SeededIds::new(24);
    let clock = clock();
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store.append(stream, 0, envelopes).await.expect("append");

    let (queue, lease_id): (String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT queue, lease_id FROM event_store.dispatch_queue WHERE job_id = $1",
    )
    .bind(stream.as_uuid())
    .fetch_one(&pool)
    .await
    .expect("dispatch row exists");
    assert_eq!(queue, "default");
    assert!(lease_id.is_none());

    let outbox: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.outbox")
        .fetch_one(&pool)
        .await
        .expect("outbox count");
    assert_eq!(outbox, 1, "enqueued event rides the outbox");

    // cancel ⇒ row removed, second outbox entry — same transaction contract
    let stream_envs = store.load(stream).await.expect("load");
    let job = Job::from_events(&stream_envs).expect("fold");
    let cancel = wrap_events(
        &ids, &clock, stream, job.version, ids.correlation_id(), None, None,
        vec![JobEvent::Cancelled { reason: None }],
    );
    store.append(stream, job.version, cancel).await.expect("cancel");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event_store.dispatch_queue WHERE job_id = $1")
            .bind(stream.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(rows, 0, "terminal ⇒ undispatchable");
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p koine-store-postgres`
Expected: compile error (`PostgresEventStore` missing).

- [ ] **Step 3: Implement `rows.rs`**

```rust
//! Row ↔ envelope mapping (ADR 0010 encoding over ADR 0012 columns).

use chrono::{DateTime, Utc};
use koine_application::ports::EventStoreError;
use koine_domain::{CorrelationId, EventEnvelope, EventId, JobEvent, JobId};
use sqlx::postgres::PgRow;
use sqlx::Row as _;
use uuid::Uuid;

/// Maps any sqlx error into the port's backend error.
pub(crate) fn db(e: sqlx::Error) -> EventStoreError {
    EventStoreError::Backend(format!("db: {e}"))
}

/// Rebuilds an envelope from an `event_store.events` row.
pub(crate) fn envelope_from_row(row: &PgRow) -> Result<EventEnvelope, EventStoreError> {
    let payload: serde_json::Value = row.try_get("payload").map_err(db)?;
    let event: JobEvent = serde_json::from_value(payload)
        .map_err(|e| EventStoreError::Backend(format!("payload decode: {e}")))?;
    Ok(EventEnvelope {
        event_id: EventId::new(row.try_get::<Uuid, _>("event_id").map_err(db)?),
        stream_id: JobId::new(row.try_get::<Uuid, _>("stream_id").map_err(db)?),
        version: u64::try_from(row.try_get::<i64, _>("version").map_err(db)?)
            .map_err(|_| EventStoreError::Backend("negative version".into()))?,
        recorded_at: row.try_get::<DateTime<Utc>, _>("recorded_at").map_err(db)?,
        correlation_id: CorrelationId::new(
            row.try_get::<Uuid, _>("correlation_id").map_err(db)?,
        ),
        causation_id: row
            .try_get::<Option<Uuid>, _>("causation_id")
            .map_err(db)?
            .map(EventId::new),
        traceparent: row.try_get("traceparent").map_err(db)?,
        schema_version: u16::try_from(row.try_get::<i16, _>("schema_version").map_err(db)?)
            .map_err(|_| EventStoreError::Backend("negative schema_version".into()))?,
        event,
    })
}
```

- [ ] **Step 4: Implement `store.rs`**

```rust
//! `PostgresEventStore`: append, dispatch projection, and outbox in ONE
//! transaction (ADRs 0006/0011/0012). A failed transaction leaves nothing —
//! the same contract the in-memory store proves with one mutex hold.

use std::future::Future;

use koine_application::ports::{EventStore, EventStoreError};
use koine_domain::{EventEnvelope, Job, JobId, JobState};
use sqlx::{PgPool, Postgres, Transaction};

use crate::rows::{db, envelope_from_row};

/// Event store over Postgres.
pub struct PostgresEventStore {
    pool: PgPool,
}

impl PostgresEventStore {
    /// Wraps a migrated pool (see [`crate::connect_pool`]).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn is_version_conflict(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::Database(db_err)
            if db_err.code().as_deref() == Some("23505")
                && db_err.constraint() == Some("events_stream_version_unique")
    )
}

/// Loads a stream's envelopes inside the transaction, version order.
pub(crate) async fn load_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    stream: JobId,
) -> Result<Vec<EventEnvelope>, EventStoreError> {
    let rows = sqlx::query(
        "SELECT stream_id, version, event_id, event_type, schema_version, payload, \
         correlation_id, causation_id, traceparent, recorded_at \
         FROM event_store.events WHERE stream_id = $1 ORDER BY version",
    )
    .bind(stream.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(db)?;
    rows.iter().map(envelope_from_row).collect()
}

/// The append composite: version check, event + outbox inserts, fold
/// validation, dispatch projection — caller owns commit/rollback.
pub(crate) async fn append_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    stream: JobId,
    expected_version: u64,
    envelopes: &[EventEnvelope],
) -> Result<Job, EventStoreError> {
    let current: Option<i64> =
        sqlx::query_scalar("SELECT max(version) FROM event_store.events WHERE stream_id = $1")
            .bind(stream.as_uuid())
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
    let current = u64::try_from(current.unwrap_or(0))
        .map_err(|_| EventStoreError::Backend("negative stream version".into()))?;
    if current != expected_version {
        return Err(EventStoreError::VersionConflict { stream, expected: expected_version });
    }
    let mut next = current;
    for envelope in envelopes {
        next += 1;
        if envelope.version != next || envelope.stream_id != stream {
            return Err(EventStoreError::Backend(format!(
                "malformed envelope batch for {stream}"
            )));
        }
    }
    for envelope in envelopes {
        let payload = serde_json::to_value(&envelope.event)
            .map_err(|e| EventStoreError::Backend(format!("payload encode: {e}")))?;
        let inserted = sqlx::query(
            "INSERT INTO event_store.events \
             (stream_id, version, event_id, event_type, schema_version, payload, \
              correlation_id, causation_id, traceparent, recorded_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(stream.as_uuid())
        .bind(i64::try_from(envelope.version).map_err(|_| {
            EventStoreError::Backend("version exceeds i64".into())
        })?)
        .bind(envelope.event_id.as_uuid())
        .bind(envelope.event.kind())
        .bind(i16::try_from(envelope.schema_version).map_err(|_| {
            EventStoreError::Backend("schema_version exceeds i16".into())
        })?)
        .bind(&payload)
        .bind(envelope.correlation_id.as_uuid())
        .bind(envelope.causation_id.map(|c| c.as_uuid()))
        .bind(envelope.traceparent.as_deref())
        .bind(envelope.recorded_at)
        .execute(&mut **tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(e) if is_version_conflict(&e) => {
                return Err(EventStoreError::VersionConflict {
                    stream,
                    expected: expected_version,
                });
            }
            Err(e) => return Err(db(e)),
        }
        let envelope_json = serde_json::to_value(envelope)
            .map_err(|e| EventStoreError::Backend(format!("envelope encode: {e}")))?;
        sqlx::query(
            "INSERT INTO event_store.outbox (event_id, stream_id, envelope) \
             VALUES ($1, $2, $3)",
        )
        .bind(envelope.event_id.as_uuid())
        .bind(stream.as_uuid())
        .bind(&envelope_json)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    }
    let stream_envelopes = load_in_tx(tx, stream).await?;
    let job = Job::from_events(&stream_envelopes)
        .map_err(|e| EventStoreError::Backend(format!("stream does not fold: {e}")))?;
    project_in_tx(tx, &job).await?;
    Ok(job)
}

/// Re-derives the job's dispatch row from folded state (rebuildable
/// projection — identical contract to the memory store's `project_locked`).
pub(crate) async fn project_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    job: &Job,
) -> Result<(), EventStoreError> {
    match &job.state {
        JobState::Pending { not_before } => {
            sqlx::query(
                "INSERT INTO event_store.dispatch_queue \
                 (job_id, queue, priority, not_before) VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (job_id) DO UPDATE SET \
                 queue = EXCLUDED.queue, priority = EXCLUDED.priority, \
                 not_before = EXCLUDED.not_before, \
                 lease_id = NULL, worker_id = NULL, lease_expires_at = NULL",
            )
            .bind(job.id.as_uuid())
            .bind(job.queue.as_str())
            .bind(job.priority.0)
            .bind(*not_before)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        }
        JobState::Leased { worker, lease, expires_at }
        | JobState::Running { worker, lease, expires_at } => {
            sqlx::query(
                "INSERT INTO event_store.dispatch_queue \
                 (job_id, queue, priority, not_before, lease_id, worker_id, lease_expires_at) \
                 VALUES ($1, $2, $3, NULL, $4, $5, $6) \
                 ON CONFLICT (job_id) DO UPDATE SET \
                 queue = EXCLUDED.queue, priority = EXCLUDED.priority, not_before = NULL, \
                 lease_id = EXCLUDED.lease_id, worker_id = EXCLUDED.worker_id, \
                 lease_expires_at = EXCLUDED.lease_expires_at",
            )
            .bind(job.id.as_uuid())
            .bind(job.queue.as_str())
            .bind(job.priority.0)
            .bind(lease.as_uuid())
            .bind(worker.as_str())
            .bind(*expires_at)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        }
        JobState::Succeeded
        | JobState::Parked { .. }
        | JobState::Cancelled
        | JobState::Suspended
        | JobState::AwaitingApproval { .. } => {
            sqlx::query("DELETE FROM event_store.dispatch_queue WHERE job_id = $1")
                .bind(job.id.as_uuid())
                .execute(&mut **tx)
                .await
                .map_err(db)?;
        }
    }
    Ok(())
}

impl EventStore for PostgresEventStore {
    fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> impl Future<Output = Result<(), EventStoreError>> + Send {
        async move {
            let mut tx = self.pool.begin().await.map_err(db)?;
            append_in_tx(&mut tx, stream, expected_version, &envelopes).await?;
            tx.commit().await.map_err(db)
        }
    }

    fn load(
        &self,
        stream: JobId,
    ) -> impl Future<Output = Result<Vec<EventEnvelope>, EventStoreError>> + Send {
        async move {
            let rows = sqlx::query(
                "SELECT stream_id, version, event_id, event_type, schema_version, payload, \
                 correlation_id, causation_id, traceparent, recorded_at \
                 FROM event_store.events WHERE stream_id = $1 ORDER BY version",
            )
            .bind(stream.as_uuid())
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
            if rows.is_empty() {
                return Err(EventStoreError::StreamNotFound(stream));
            }
            rows.iter().map(envelope_from_row).collect()
        }
    }
}
```

Wire `lib.rs`: add `pub mod dispatcher;` later (Task 5) — for now:

```rust
mod rows;
pub mod store;

pub use store::PostgresEventStore;
```

(placed after the existing `connect_pool`/`MIGRATOR` items).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p koine-store-postgres && cargo clippy -p koine-store-postgres --all-targets -- -D warnings && cargo fmt --all`
Expected: 5 tests PASS (smoke + 4 contract).

- [ ] **Step 6: Commit**

```bash
git add crates/koine-store-postgres
git commit -m "feat(store-postgres): add transactional event store with projection"
```

---

### Task 5: `PostgresDispatcher` — SKIP LOCKED claim

**Files:**
- Create: `crates/koine-store-postgres/src/dispatcher.rs`, `crates/koine-store-postgres/tests/dispatcher.rs`
- Modify: `crates/koine-store-postgres/src/lib.rs` (add `pub mod dispatcher;` + `pub use dispatcher::PostgresDispatcher;`)

**Interfaces:**
- Consumes: `append_in_tx`/`load_in_tx` (Task 4), `Dispatcher` port, `lineage_of`/`wrap_events`, domain `Job`.
- Produces: `PostgresDispatcher<G: IdGenerator, C: Clock>::new(pool: PgPool, ids: Arc<G>, clock: Arc<C>)` implementing `Dispatcher` — the ADR 0011-b composite as one SQL transaction.

- [ ] **Step 1: Write the failing tests** (`tests/dispatcher.rs` — same header convention; helpers mirror the 1A memory-dispatcher fixture but against `support::pg()`; write them concretely):

```rust
//! Ring-3 dispatcher tests: the ADR 0011 claim composite over real SQL.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use koine_application::ports::{Clock as _, Dispatcher as _, EventStore as _, IdGenerator as _};
use koine_application::wrap_events;
use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::{PostgresDispatcher, PostgresEventStore};

struct Fx {
    _guard: testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
    store: PostgresEventStore,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    dispatcher: PostgresDispatcher<SeededIds, FixedClock>,
    queue: QueueName,
    worker: WorkerId,
}

async fn fx() -> Fx {
    use chrono::TimeZone as _;
    let (guard, pool) = support::pg().await;
    let ids = Arc::new(SeededIds::new(31));
    let clock = Arc::new(FixedClock::at(
        chrono::Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"),
    ));
    Fx {
        _guard: guard,
        store: PostgresEventStore::new(pool.clone()),
        dispatcher: PostgresDispatcher::new(pool, Arc::clone(&ids), Arc::clone(&clock)),
        ids,
        clock,
        queue: QueueName::new("default").expect("q"),
        worker: WorkerId::new("w1").expect("w"),
    }
}

async fn enqueue(f: &Fx, priority: i16, not_before_secs: Option<u64>) -> JobId {
    let stream = f.ids.job_id();
    let now = f.clock.now();
    let not_before = not_before_secs
        .map(|s| now + chrono::TimeDelta::seconds(i64::try_from(s).expect("secs")));
    let event = Job::initial_event(
        f.queue.clone(),
        serde_json::json!({"job": stream.to_string()}),
        Priority(priority),
        RetryPolicy::default(),
        not_before,
    );
    let envs = wrap_events(
        f.ids.as_ref(), f.clock.as_ref(), stream, 0, f.ids.correlation_id(), None, None,
        vec![event],
    );
    f.store.append(stream, 0, envs).await.expect("enqueue");
    stream
}

#[tokio::test]
async fn claims_by_priority_then_fifo_and_appends_leased() {
    let f = fx().await;
    let low_first = enqueue(&f, 0, None).await;
    let high = enqueue(&f, 9, None).await;
    let ttl = Duration::from_secs(30);

    let first = f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim");
    assert_eq!(first.expect("job").job_id, high, "priority first");

    let second = f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim");
    let second = second.expect("job");
    assert_eq!(second.job_id, low_first, "then FIFO");

    let stream = f.store.load(second.job_id).await.expect("load");
    assert_eq!(stream[1].event.kind(), "leased");
    assert_eq!(stream[1].correlation_id, stream[0].correlation_id, "lineage carried");

    assert!(
        f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim").is_none(),
        "drained"
    );
}

#[tokio::test]
async fn respects_not_before_and_lease_expiry() {
    let f = fx().await;
    enqueue(&f, 0, Some(60)).await;
    let ttl = Duration::from_secs(30);
    assert!(f.dispatcher.lease_next(&f.queue, &f.worker, ttl).await.expect("claim").is_none());
    f.clock.advance(Duration::from_secs(61));
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, ttl)
        .await
        .expect("claim")
        .expect("eligible now");

    let now = f.clock.now();
    assert!(f.dispatcher.expired(now).await.expect("expired").is_empty());
    assert!(f.dispatcher.extend_lease(claimed.lease, ttl).await.expect("hb"));
    f.clock.advance(Duration::from_secs(31));
    let now = f.clock.now();
    assert!(f.dispatcher.expired(now).await.expect("expired").is_empty(), "extended");
    f.clock.advance(Duration::from_secs(31));
    let now = f.clock.now();
    assert_eq!(f.dispatcher.expired(now).await.expect("expired"), vec![claimed.job_id]);
    assert!(!f.dispatcher.extend_lease(claimed.lease, ttl).await.expect("hb"), "expired refuses");
}

#[tokio::test]
async fn concurrent_claims_get_distinct_jobs() {
    let f = fx().await;
    let a = enqueue(&f, 0, None).await;
    let b = enqueue(&f, 0, None).await;
    let w2 = WorkerId::new("w2").expect("w");
    let ttl = Duration::from_secs(30);
    let (r1, r2) = tokio::join!(
        f.dispatcher.lease_next(&f.queue, &f.worker, ttl),
        f.dispatcher.lease_next(&f.queue, &w2, ttl),
    );
    let j1 = r1.expect("claim 1").expect("job 1").job_id;
    let j2 = r2.expect("claim 2").expect("job 2").job_id;
    assert_ne!(j1, j2, "SKIP LOCKED: no double-claim");
    let mut got = [j1, j2];
    got.sort();
    let mut want = [a, b];
    want.sort();
    assert_eq!(got, want);
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p koine-store-postgres --test dispatcher`
Expected: compile error (`PostgresDispatcher` missing).

- [ ] **Step 3: Implement `dispatcher.rs`**

```rust
//! `PostgresDispatcher`: the ADR 0011-b claim composite as one SQL
//! transaction — `SELECT … FOR UPDATE SKIP LOCKED` on the dispatch partial
//! index, domain-validated lease, event + outbox + row update, commit.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use koine_application::lineage_of;
use koine_application::ports::{
    Clock, DispatchError, Dispatcher, EventStoreError, IdGenerator, LeasedJob,
};
use koine_application::wrap_events;
use koine_domain::{Job, JobEvent, JobId, LeaseId, QueueName, WorkerId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::rows::db;
use crate::store::{append_in_tx, load_in_tx};

/// Dispatcher over Postgres.
pub struct PostgresDispatcher<G, C> {
    pool: PgPool,
    ids: Arc<G>,
    clock: Arc<C>,
}

impl<G: IdGenerator, C: Clock> PostgresDispatcher<G, C> {
    /// New dispatcher over a migrated pool.
    #[must_use]
    pub fn new(pool: PgPool, ids: Arc<G>, clock: Arc<C>) -> Self {
        Self { pool, ids, clock }
    }

    async fn claim(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeasedJob>, DispatchError> {
        let now = self.clock.now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let picked: Option<(Uuid,)> = sqlx::query_as(
            "SELECT job_id FROM event_store.dispatch_queue \
             WHERE queue = $1 AND lease_id IS NULL \
               AND (not_before IS NULL OR not_before <= $2) \
             ORDER BY priority DESC, seq \
             LIMIT 1 FOR UPDATE SKIP LOCKED",
        )
        .bind(queue.as_str())
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some((job_uuid,)) = picked else {
            return Ok(None);
        };
        let job_id = JobId::new(job_uuid);
        let stream = load_in_tx(&mut tx, job_id).await?;
        let job = Job::from_events(&stream)
            .map_err(|e| EventStoreError::Backend(format!("fold: {e}")))?;
        let lease = self.ids.lease_id();
        let event = job
            .lease(worker.clone(), lease, now, ttl)
            .map_err(|e| EventStoreError::Backend(format!("index/state drift: {e}")))?;
        let (correlation_id, causation_id, traceparent) = lineage_of(&stream);
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
        let JobEvent::Leased { expires_at, .. } = envelopes[0].event else {
            return Err(EventStoreError::Backend("lease produced non-lease".into()).into());
        };
        let folded = append_in_tx(&mut tx, job_id, job.version, &envelopes).await?;
        tx.commit().await.map_err(db)?;
        Ok(Some(LeasedJob {
            job_id,
            queue: folded.queue,
            payload: folded.payload,
            attempt: folded.attempt,
            lease,
            expires_at,
            correlation_id,
            traceparent,
        }))
    }
}

impl<G: IdGenerator, C: Clock> Dispatcher for PostgresDispatcher<G, C> {
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send {
        self.claim(queue, worker, ttl)
    }

    fn extend_lease(
        &self,
        lease: LeaseId,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool, DispatchError>> + Send {
        async move {
            let now = self.clock.now();
            let Ok(delta) = chrono::TimeDelta::from_std(ttl) else {
                return Err(DispatchError::Backend("ttl out of range".into()));
            };
            let deadline = now + delta;
            let updated = sqlx::query(
                "UPDATE event_store.dispatch_queue SET lease_expires_at = $1 \
                 WHERE lease_id = $2 AND lease_expires_at > $3",
            )
            .bind(deadline)
            .bind(lease.as_uuid())
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(db)?;
            Ok(updated.rows_affected() > 0)
        }
    }

    fn expired(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<JobId>, DispatchError>> + Send {
        async move {
            let rows: Vec<(Uuid,)> = sqlx::query_as(
                "SELECT job_id FROM event_store.dispatch_queue \
                 WHERE lease_id IS NOT NULL AND lease_expires_at <= $1 \
                 ORDER BY job_id",
            )
            .bind(now)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
            Ok(rows.into_iter().map(|(id,)| JobId::new(id)).collect())
        }
    }
}
```

(Note: `db` returns `EventStoreError`; where a `DispatchError` is needed it converts via `#[from]` — add `.map_err(DispatchError::from)`/`?` as the compiler directs, keeping semantics identical.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p koine-store-postgres && cargo clippy -p koine-store-postgres --all-targets -- -D warnings && cargo fmt --all`
Expected: 8 tests PASS (incl. the SKIP LOCKED concurrency proof).

- [ ] **Step 5: Commit**

```bash
git add crates/koine-store-postgres
git commit -m "feat(store-postgres): add skip-locked dispatcher"
```

---

### Task 6: Outbox relay — claim-delete batches + `EventSink` port

**Files:**
- Create: `crates/koine-store-postgres/src/relay.rs`, `crates/koine-store-postgres/tests/outbox.rs`
- Modify: `crates/koine-application/src/ports.rs` (+`EventSink`, `SinkError`, `RelayError`), `crates/koine-application/src/lib.rs` (re-exports), `crates/koine-store-postgres/src/lib.rs` (`pub mod relay;` + re-export)

**Interfaces:**
- Consumes: outbox rows written by Task 4's `append_in_tx`.
- Produces: `EventSink { deliver(&self, envelopes: &[EventEnvelope]) -> impl Future<Output = Result<(), SinkError>> + Send }`; `SinkError::Failed(String)`; `RelayError::{Sink(SinkError), Backend(String)}`; `PostgresOutboxRelay::new(pool)` with `relay_once<S: EventSink>(&self, sink: &S, batch: i64) -> Result<u32, RelayError>` — Task 9's dev-loop drives it.

- [ ] **Step 1: Add the port types** (`ports.rs`, after the dispatcher section)

```rust
/// Errors a sink may return. A failed batch is rolled back and redelivered
/// on a later relay pass — sinks must be idempotent (at-least-once).
#[derive(Debug, Error)]
pub enum SinkError {
    /// Delivery failed; the batch will be retried.
    #[error("sink: {0}")]
    Failed(String),
}

/// Errors from an outbox relay pass.
#[derive(Debug, Error)]
pub enum RelayError {
    /// The sink rejected the batch (rolled back, will retry).
    #[error(transparent)]
    Sink(#[from] SinkError),
    /// Adapter/backend failure.
    #[error("backend: {0}")]
    Backend(String),
}

/// Consumer of relayed envelopes: read projections (phase 3), logging and
/// counting sinks today (ADR 0012).
pub trait EventSink: Send + Sync {
    /// Processes one ordered batch. Erring rolls the whole batch back.
    fn deliver(
        &self,
        envelopes: &[EventEnvelope],
    ) -> impl Future<Output = Result<(), SinkError>> + Send;
}
```

Re-export in `lib.rs`: add `EventSink, RelayError, SinkError` to the `pub use ports::{…}` list.

- [ ] **Step 2: Write the failing tests** (`tests/outbox.rs`)

```rust
//! Ring-3 outbox relay tests: claim-delete semantics (ADR 0012).
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Mutex;

use koine_application::ports::{EventSink, EventStore as _, IdGenerator as _, SinkError};
use koine_application::wrap_events;
use koine_domain::{EventEnvelope, Job, Priority, QueueName, RetryPolicy};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::{PostgresEventStore, PostgresOutboxRelay};

struct Collecting(Mutex<Vec<String>>);
impl EventSink for Collecting {
    async fn deliver(&self, envelopes: &[EventEnvelope]) -> Result<(), SinkError> {
        let mut seen = self.0.lock().expect("lock");
        seen.extend(envelopes.iter().map(|e| format!("{}:{}", e.stream_id, e.event.kind())));
        Ok(())
    }
}

struct Failing;
impl EventSink for Failing {
    async fn deliver(&self, _: &[EventEnvelope]) -> Result<(), SinkError> {
        Err(SinkError::Failed("down".into()))
    }
}

fn clock() -> FixedClock {
    use chrono::TimeZone as _;
    FixedClock::at(chrono::Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0).single().expect("ts"))
}

async fn enqueue(store: &PostgresEventStore, ids: &SeededIds, clock: &FixedClock) -> koine_domain::JobId {
    let stream = ids.job_id();
    let event = Job::initial_event(
        QueueName::new("default").expect("q"),
        serde_json::json!({}),
        Priority(0),
        RetryPolicy::default(),
        None,
    );
    let envs = wrap_events(ids, clock, stream, 0, ids.correlation_id(), None, None, vec![event]);
    store.append(stream, 0, envs).await.expect("append");
    stream
}

#[tokio::test]
async fn relays_in_order_and_deletes_on_success() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool.clone());
    let relay = PostgresOutboxRelay::new(pool.clone());
    let ids = SeededIds::new(41);
    let clock = clock();
    let a = enqueue(&store, &ids, &clock).await;
    let b = enqueue(&store, &ids, &clock).await;

    let sink = Collecting(Mutex::new(Vec::new()));
    assert_eq!(relay.relay_once(&sink, 10).await.expect("relay"), 2);
    let seen = sink.0.lock().expect("lock").clone();
    assert_eq!(seen, vec![format!("{a}:enqueued"), format!("{b}:enqueued")], "outbox order");

    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.outbox")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(left, 0, "delivered rows deleted");
    assert_eq!(relay.relay_once(&sink, 10).await.expect("relay"), 0, "drained");
}

#[tokio::test]
async fn sink_failure_rolls_back_for_redelivery() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool.clone());
    let relay = PostgresOutboxRelay::new(pool.clone());
    let ids = SeededIds::new(42);
    let clock = clock();
    enqueue(&store, &ids, &clock).await;

    let err = relay.relay_once(&Failing, 10).await.expect_err("sink down");
    assert!(matches!(err, koine_application::RelayError::Sink(_)));
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.outbox")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(left, 1, "failed batch stays for redelivery");

    let sink = Collecting(Mutex::new(Vec::new()));
    assert_eq!(relay.relay_once(&sink, 10).await.expect("relay"), 1, "redelivered");
}
```

(Note: `async fn` in an impl of an RPITIT trait is allowed and infers the `+ Send` bound from the trait declaration — if the compiler objects, desugar to `fn … -> impl Future … + Send { async move { … } }`.)

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p koine-store-postgres --test outbox`
Expected: compile error.

- [ ] **Step 4: Implement `relay.rs`**

```rust
//! `PostgresOutboxRelay`: claims ordered batches with SKIP LOCKED, delivers
//! to a sink, deletes on success (ADR 0012 — claim-delete, no positions).

use koine_application::ports::{EventSink, RelayError};
use koine_domain::EventEnvelope;
use sqlx::{PgPool, Row as _};

/// Single-instance outbox relay (concurrency arrives with phase-3 consumers).
pub struct PostgresOutboxRelay {
    pool: PgPool,
}

impl PostgresOutboxRelay {
    /// New relay over a migrated pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// One pass: claim up to `batch` rows in `outbox_seq` order, deliver,
    /// delete. Returns rows delivered. Sink failure rolls the claim back.
    ///
    /// # Errors
    /// [`RelayError::Sink`] when the sink rejects (batch redelivered later);
    /// [`RelayError::Backend`] on database failure.
    pub async fn relay_once<S: EventSink>(
        &self,
        sink: &S,
        batch: i64,
    ) -> Result<u32, RelayError> {
        let backend = |e: sqlx::Error| RelayError::Backend(format!("db: {e}"));
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let rows = sqlx::query(
            "SELECT outbox_seq, envelope FROM event_store.outbox \
             ORDER BY outbox_seq LIMIT $1 FOR UPDATE SKIP LOCKED",
        )
        .bind(batch)
        .fetch_all(&mut *tx)
        .await
        .map_err(backend)?;
        if rows.is_empty() {
            return Ok(0);
        }
        let mut seqs: Vec<i64> = Vec::with_capacity(rows.len());
        let mut envelopes: Vec<EventEnvelope> = Vec::with_capacity(rows.len());
        for row in &rows {
            seqs.push(row.try_get("outbox_seq").map_err(backend)?);
            let value: serde_json::Value = row.try_get("envelope").map_err(backend)?;
            envelopes.push(
                serde_json::from_value(value)
                    .map_err(|e| RelayError::Backend(format!("envelope decode: {e}")))?,
            );
        }
        sink.deliver(&envelopes).await?;
        sqlx::query("DELETE FROM event_store.outbox WHERE outbox_seq = ANY($1)")
            .bind(&seqs)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(u32::try_from(envelopes.len()).unwrap_or(u32::MAX))
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p koine-store-postgres && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`
Expected: 10 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/koine-application crates/koine-store-postgres
git commit -m "feat(store-postgres): add claim-delete outbox relay and sink port"
```

---

### Task 7: Ring-3 lifecycle — the crash-recovery story on real SQL

**Files:**
- Create: `crates/koine-store-postgres/tests/lifecycle.rs`

**Interfaces:**
- Consumes: use cases (1A + Task 3 signatures), Postgres adapters (Tasks 4–5).
- Produces: the epic's "enqueue→lease→ack/fail→retry→park works against both stores" evidence.

- [ ] **Step 1: Write the suite** — mirror the 1A ring-2 scenarios against Postgres. Same file-header convention; fixture like Task 5's `fx()` plus use-case helpers exactly as `crates/koine-store-memory/tests/lifecycle.rs` builds them (`EnqueueJob`, `WorkerAck`, `SweepExpiredLeases` — read that file and mirror; the ONLY differences: `world()` uses `support::pg().await` + Postgres adapter types, and `enqueue` handles the Task 3 `EnqueueError`). Write these four tests concretely (assert the same stories):

1. `happy_path_records_the_full_story` — kinds == `["enqueued","leased","started","succeeded"]`, then queue drained.
2. `worker_crash_is_recovered_by_the_sweep` — tight policy (3/1s/2s); claim; advance 31s; sweep==1; kinds == `[…,"lease_expired","retry_scheduled"]`; advance 3s; re-claim `attempt == 1`.
3. `late_ack_after_expiry_is_recorded_never_lost` — after sweep, stale succeed → `AckOutcome::Conflict` + trailing `late_ack_conflict`; then normal re-claim → start → succeed ends `succeeded`.
4. `repeated_crashes_exhaust_into_parked` — max_attempts 1; crash; sweep; last kind `parked`; never re-claimable.

- [ ] **Step 2: Run to green**

Run: `cargo test -p koine-store-postgres --test lifecycle`
Expected: 4 PASS (first run compiles + pulls nothing new).

- [ ] **Step 3: Full gate + commit**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all`

```bash
git add crates/koine-store-postgres
git commit -m "test(store-postgres): mirror lifecycle crash scenarios on ring 3"
```

---

### Task 8: Dispatch rebuild from the log — the replay guarantee

**Files:**
- Modify: `crates/koine-store-postgres/src/store.rs` (+`rebuild_dispatch`), `crates/koine-store-postgres/src/lib.rs` (re-export)
- Create: `crates/koine-store-postgres/tests/replay.rs`

**Interfaces:**
- Consumes: `project_in_tx`, `load_in_tx`.
- Produces: `pub async fn rebuild_dispatch(pool: &PgPool) -> Result<u32, EventStoreError>` — the epic's "every projection replays from event zero to an identical state" made executable (and a real ops tool).

- [ ] **Step 1: Write the failing test** (`tests/replay.rs`, standard header + `mod support;`)

```rust
#[tokio::test]
async fn dispatch_queue_rebuilds_identically_from_the_log() {
    let (_guard, pool) = support::pg().await;
    // build a mixed world: one pending, one scheduled, one leased, one done
    // (reuse Task 5's fixture helpers inline: enqueue x4, claim one, complete one)
    // … [construct exactly as in tests/dispatcher.rs: fx()-style setup]
    // snapshot rows (ordered by queue, priority DESC, seq):
    let before: Vec<(uuid::Uuid, String, i16, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT job_id, queue, priority, lease_id FROM event_store.dispatch_queue \
         ORDER BY queue, priority DESC, seq",
    )
    .fetch_all(&pool)
    .await
    .expect("snapshot");

    sqlx::query("TRUNCATE event_store.dispatch_queue")
        .execute(&pool)
        .await
        .expect("truncate");

    let rebuilt = koine_store_postgres::rebuild_dispatch(&pool).await.expect("rebuild");
    assert_eq!(rebuilt as usize, before.len());

    let after: Vec<(uuid::Uuid, String, i16, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT job_id, queue, priority, lease_id FROM event_store.dispatch_queue \
         ORDER BY queue, priority DESC, seq",
    )
    .fetch_all(&pool)
    .await
    .expect("resnapshot");
    // seq values are re-minted; ORDER and every other column must match
    assert_eq!(after, before, "projection replays from zero to identical state");
}
```

(Fill the elided setup concretely from the dispatcher fixture: 4 enqueues with distinct priorities, one `lease_next`, one full succeed via `WorkerAck` — the implementer copies those proven helpers into this file.)

- [ ] **Step 2: Implement `rebuild_dispatch`** (append to `store.rs`)

```rust
/// Rebuilds `dispatch_queue` from the event log: folds every stream in
/// first-appearance order and re-projects. The projection is derived state
/// (ADR 0006) — this is both the replay guarantee's proof and an ops tool.
///
/// # Errors
/// Database failure, or a stream that no longer folds (data corruption).
pub async fn rebuild_dispatch(pool: &PgPool) -> Result<u32, EventStoreError> {
    let mut tx = pool.begin().await.map_err(db)?;
    let streams: Vec<(uuid::Uuid,)> = sqlx::query_as(
        "SELECT stream_id FROM event_store.events \
         GROUP BY stream_id ORDER BY min(global_seq)",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(db)?;
    let mut projected = 0u32;
    for (stream_uuid,) in streams {
        let stream = JobId::new(stream_uuid);
        let envelopes = load_in_tx(&mut tx, stream).await?;
        let job = Job::from_events(&envelopes)
            .map_err(|e| EventStoreError::Backend(format!("stream {stream}: {e}")))?;
        let dispatchable = matches!(
            job.state,
            JobState::Pending { .. } | JobState::Leased { .. } | JobState::Running { .. }
        );
        project_in_tx(&mut tx, &job).await?;
        if dispatchable {
            projected += 1;
        }
    }
    tx.commit().await.map_err(db)?;
    Ok(projected)
}
```

Re-export from `lib.rs`: `pub use store::{rebuild_dispatch, PostgresEventStore};`

- [ ] **Step 3: Run + gate + commit**

Run: `cargo test -p koine-store-postgres --test replay && make ci`

```bash
git add crates/koine-store-postgres
git commit -m "feat(store-postgres): add dispatch rebuild proving replay from zero"
```

---

### Task 9: `koine-server dev-loop` — the product exercise

**Files:**
- Create: `crates/koine-server/src/runtime.rs`, `crates/koine-server/src/dev_loop.rs`
- Modify: `crates/koine-server/src/main.rs`, `crates/koine-server/Cargo.toml` (verify Task 1 deps landed)

**Interfaces:**
- Consumes: everything.
- Produces: `SystemClock` (Clock via `Utc::now`), `UuidV7Ids` (IdGenerator via `Uuid::now_v7`, jitter from v7 entropy); `koine-server dev-loop` subcommand — DoD item 2's end-to-end product exercise.

- [ ] **Step 1: `runtime.rs`**

```rust
//! Production `Clock`/`IdGenerator` implementations (composition root).

use chrono::{DateTime, Utc};
use koine_application::ports::{Clock, IdGenerator};
use koine_domain::{CorrelationId, EventId, JobId, LeaseId};
use uuid::Uuid;

/// Wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// UUIDv7 identity source (ADR 0010).
pub struct UuidV7Ids;

impl IdGenerator for UuidV7Ids {
    fn job_id(&self) -> JobId {
        JobId::new(Uuid::now_v7())
    }
    fn event_id(&self) -> EventId {
        EventId::new(Uuid::now_v7())
    }
    fn lease_id(&self) -> LeaseId {
        LeaseId::new(Uuid::now_v7())
    }
    fn correlation_id(&self) -> CorrelationId {
        CorrelationId::new(Uuid::now_v7())
    }
    fn jitter_seed(&self) -> u64 {
        // High-entropy per the port contract: fold both UUID halves.
        let bits = Uuid::now_v7().as_u128();
        #[allow(clippy::cast_possible_truncation)] // intentional fold of both halves
        {
            (bits as u64) ^ ((bits >> 64) as u64)
        }
    }
}
```

- [ ] **Step 2: `dev_loop.rs`** — the full cycle against real Postgres. Structure (write it concretely; ~120 lines):

```rust
//! `dev-loop`: exercises the entire 1B stack end-to-end against a real
//! database — enqueue, worker loop, sweep, outbox relay — and prints each
//! job's recorded story (DoD "exercised as a product").

use std::sync::Arc;
use std::time::Duration;

use koine_application::ports::{EventSink, EventStore as _, SinkError};
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::lease::LeaseNextJob;
use koine_application::use_cases::sweep::SweepExpiredLeases;
use koine_application::use_cases::worker_ack::WorkerAck;
use koine_application::Lineage;
use koine_domain::{EventEnvelope, JobError, JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_postgres::{
    connect_pool, PostgresDispatcher, PostgresEventStore, PostgresOutboxRelay,
};

use crate::runtime::{SystemClock, UuidV7Ids};

struct PrintingSink;
impl EventSink for PrintingSink {
    async fn deliver(&self, envelopes: &[EventEnvelope]) -> Result<(), SinkError> {
        for env in envelopes {
            println!("  [outbox→sink] {} v{} {}", env.stream_id, env.version, env.event.kind());
        }
        Ok(())
    }
}

/// Runs the loop; returns an error message on failure.
pub async fn run(database_url: &str) -> Result<(), String> {
    // 1. connect + migrate; build store/dispatcher/relay with SystemClock/UuidV7Ids
    // 2. enqueue 3 jobs on queue "dev": two plain, one {"flaky": true}
    //    (short lease ttl 2s, RetryPolicy { max_attempts: 3, base_delay 500ms, max_delay 2s })
    // 3. spawn worker task: loop { lease_next("dev") → start → if payload.flaky
    //    && attempt == 0 { fail(retryable io error) } else { succeed };
    //    sleep 100ms when idle }  — worker deliberately SKIPS acking the 3rd
    //    job's first lease once (simulated crash) by matching a "crashy" flag
    //    on the second plain job: lease it, then drop it without ack.
    // 4. main loop every 300ms: sweep.execute(), relay.relay_once(&PrintingSink, 64)
    // 5. poll job states via store.load(...) kinds until all 3 terminal
    //    (succeeded) or 60s timeout → Err
    // 6. print each job's full kind story; assert (in code) that the stories
    //    contain: one plain "enqueued,leased,started,succeeded"; the crashy
    //    one contains "lease_expired"; the flaky one contains
    //    "failed,retry_scheduled" — return Err listing any missing marker.
    …
}
```

The elision above is structural narration for THIS plan only — the implementer writes every line, following the numbered comments exactly; the in-code assertions make the exercise self-verifying.

- [ ] **Step 3: `main.rs`**

```rust
//! Koiné server binary: composition root wiring adapters to the application core.

mod dev_loop;
mod runtime;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("dev-loop") => {
            let url = std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://koine:koine@localhost:5432/koine".into());
            match dev_loop::run(&url).await {
                Ok(()) => {
                    println!("dev-loop: all jobs terminal — stack exercised end-to-end");
                    std::process::ExitCode::SUCCESS
                }
                Err(msg) => {
                    eprintln!("dev-loop failed: {msg}");
                    std::process::ExitCode::FAILURE
                }
            }
        }
        _ => {
            println!("koine-server 0.1.0 — commands: dev-loop");
            std::process::ExitCode::SUCCESS
        }
    }
}
```

(Delete the phase-0 stub `use … as _;` lines — the deps are real now.)

- [ ] **Step 4: Run the product exercise** (this is DoD item 2 — capture the output)

```bash
docker run -d --name koine-devloop -e POSTGRES_PASSWORD=koine -e POSTGRES_USER=koine -e POSTGRES_DB=koine -p 55432:5432 postgres:17
sleep 3
DATABASE_URL=postgres://koine:koine@localhost:55432/koine cargo run -p koine-server -- dev-loop
docker rm -f koine-devloop
```

Expected: per-job stories printed, ending `dev-loop: all jobs terminal — stack exercised end-to-end`, exit 0. Copy the story output into the task report (it goes into the backlog evidence in Task 10).

- [ ] **Step 5: Gate + commit**

Run: `make ci`

```bash
git add crates/koine-server
git commit -m "feat(server): add dev-loop product exercise"
```

---

### Task 10: Closeout — wiki, records, phase 1 complete

**Files:**
- Create: `docs/architecture/koine-store-postgres.md`, `docs/architecture/koine-server.md`
- Create: `.apptlas/backlog/done/phase-1b-postgres-store.md`
- Move: `.apptlas/backlog/todo/retry-policy-ttl-bounds-hardening.md` → `.apptlas/backlog/done/` (check its AC boxes, fill Evidence + fidelity)
- Modify: `docs/architecture/README.md` (2 rows), `docs/architecture/overview.md` (2 crate rows), `.apptlas/epics/phase-1-event-sourced-core.md` (State), `CLAUDE.md` (header + phase log)

**Interfaces:** consumes everything; produces the record phase 2's planning builds on.

- [ ] **Step 1: Wiki pages** (What/How/Why/Boundaries; write from delivered code; docs-quality rubric applies):
- `koine-store-postgres.md` — What: production adapters (`PostgresEventStore`, `PostgresDispatcher`, `PostgresOutboxRelay`, `rebuild_dispatch`, `connect_pool`). How: one transaction per composite (append = version check + events + outbox + fold + projection; claim = SKIP LOCKED + domain lease + append); schema tables and the two partial indexes; claim-delete relay; runtime queries. Why: ADRs 0005/0006/0011/0012. Boundaries: → application + domain; requires Postgres ≥ (image used); memory store is the behavioral twin (contract tests mirror).
- `koine-server.md` — What: composition root; `SystemClock`/`UuidV7Ids`; `dev-loop`. How: wiring + tickers. Why: hexagonal composition (ADR 0003). Boundaries: the only crate that sees every adapter; grows per phase (gRPC phase 2).
- README page table: two rows `Current (phase 1B)`. overview.md: `koine-store-postgres` row → real (link page); `koine-server` row → "Composition root; dev-loop (phase 1B) — grows with each phase".

- [ ] **Step 2: Records**
- `.apptlas/backlog/done/phase-1b-postgres-store.md` (item template): Implements: spec §3 hot path + ADR 0012; epic items 8–11. AC (all checked, with verify commands): ring-3 contract parity incl. side-effect-free failures; SKIP LOCKED no-double-claim; outbox order + rollback redelivery; lifecycle mirror; `rebuild_dispatch` replay-identical; dev-loop product exercise (paste the captured story output). Fidelity: faithful; note "relay is single-instance by ADR 0012; consumer positions deferred to phase 3" + any execution deviations.
- Hardening item → done/: check AC1–AC5 with their verify evidence (tests from Tasks 2–3 + parity test names), fidelity "faithful".
- Epic State line → `- **State:** COMPLETE (2026-07-18) — 1A (rings 1–2) + 1B (Postgres, outbox, ring 3, dev-loop). Exit criteria met: rings 1–3 green; lifecycle through use cases against both stores; dispatch projection replays from zero (tests/replay.rs).`
- `CLAUDE.md`: header `**Current phase: 1 complete — next: phase 2 (data plane)**`; active plan line → (none until phase-2 plan); phase log: `- 2026-07-18 — Phase 1B complete: Postgres store, outbox relay, ring 3, dev-loop. PHASE 1 COMPLETE. Next: phase 2 plan (TLA+ model first — epic item 1).`

- [ ] **Step 3: Gate + commit**

Run: `make ci`
Expected: green.

```bash
git add docs/architecture .apptlas CLAUDE.md
git commit -m "docs: close out phase 1b — phase 1 complete"
```

---

## Not in this plan (deliberately)

- **TLC model checking + protocol properties** — phase 2 item 1 (the skeleton exists in `docs/formal/`).
- **Kani pilot** — epic item 13 stretch; evaluate during phase-2 planning.
- **Relay concurrency / consumer positions** — phase 3, per ADR 0012.
- **Benchmarks** — phase 2 (spec §7).
- **crates.io publication** — phase 2, after `manifest-cleanup-workspace-deps`.
