# Koiné Postgres Resource Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent idle fetches and best-effort presence from starving correctness-critical database work, with explicit fail-closed pool and duration configuration.

**Architecture:** Configure the operational SQLx pool explicitly, move Postgres notifications onto one dedicated reconnecting listener per server process, and make presence acquisition non-blocking with a 100 ms query budget. Correctness continues to rely on dispatch rechecks and `idle_poll`, never on notification delivery.

**Tech Stack:** Rust 1.95, Tokio broadcast/time, SQLx `PgPoolOptions`/`PgListener`, Postgres testcontainers, tonic server composition.

## Global Constraints

- Implements hardening design §§3 and 5; preserves ADR-0013 and ADR-0015.
- Defaults are `KOINE_DB_MAX_CONNECTIONS=16` and `KOINE_DB_ACQUIRE_TIMEOUT_MS=5000`.
- Presence write budget is exactly `100 ms`.
- Server connection budget is operational pool size plus one dedicated listener.
- Pool sizes down to one remain legal; zero safety values fail before connection/bind.
- No new external dependency, migration, event, or wire change.
- Complete the atomic-lease plan before starting this plan.

---

## File map

- `crates/koine-store-postgres/src/lib.rs`: public `PoolConfig` and configured connection entrypoint.
- `crates/koine-store-postgres/src/{presence,signal}.rs`: bounded ephemera adapters.
- `crates/koine-store-postgres/tests/{pool,signal}.rs`: pool, saturation, fan-out, reconnect tests.
- `crates/koine-{store-postgres,grpc}/tests/support/mod.rs`: explicit pool config and optional database URL.
- `crates/koine-server/src/{serve,dev_loop}.rs`: environment parsing/composition.
- `crates/koine-grpc/tests/grpc_e2e.rs`: async signal construction.
- `docs/architecture/{koine-store-postgres,koine-server,koine-grpc}.md` and `.env.example`: operator contract.
- `.apptlas/backlog/.../phase-2a-postgres-resource-safety.md`: slice evidence.
- `.apptlas/backlog/todo/phase-2-carryover-hardening.md`: legacy AC4 owner.

### Task 1: Open the ready resource-safety item

**Files:**

- Create then move: `.apptlas/backlog/{todo,ongoing}/phase-2a-postgres-resource-safety.md`

**Interfaces:**

- Consumes: hardening design §5 and legacy carryover AC4.
- Produces: review/evidence owner for Tasks 2–5.

- [ ] **Step 1: Create the item**

```markdown
# Bound Postgres resources on the phase-2A server

- **State:** todo
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§3, 5, 9–10; resource-hardening plan Tasks 1–5; closes phase-2-carryover-hardening AC4.

## Acceptance criteria

- [ ] AC1: `connect_pool` honors explicit non-zero max connections and acquisition timeout — *verify:* ring-3 `pool_options_are_honored` and server parse tests.
- [ ] AC2: zero TTL, idle poll, pool size, and acquisition timeout fail before startup — *verify:* `koine-server` unit tests.
- [ ] AC3: any number of idle waits share one dedicated listener and leave a size-one operational pool available — *verify:* ring-3 fan-out pressure test.
- [ ] AC4: listener reconnect preserves prompt wakeup; loss still falls back to `idle_poll` — *verify:* ring-3 reconnect and existing gRPC wakeup tests.
- [ ] AC5: saturated-pool presence returns promptly and never fails the worker request — *verify:* ring-3 `presence_skips_when_pool_is_saturated`.
- [ ] AC6: env/reference/wiki docs state defaults, constraints, total connection budget, and phase-3 relay/sink risk — *verify:* docs review and `make ci`.

## Dependencies

- Atomic lease retirement item closed.

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
```

- [ ] **Step 2: Move ready item and commit**

```bash
git mv .apptlas/backlog/todo/phase-2a-postgres-resource-safety.md .apptlas/backlog/ongoing/
git add .apptlas/backlog/ongoing/phase-2a-postgres-resource-safety.md
git commit -m "docs: open postgres resource hardening"
```

### Task 2: Make pool and startup configuration explicit

**Files:**

- Modify: `crates/koine-store-postgres/src/lib.rs`
- Create: `crates/koine-store-postgres/tests/pool.rs`
- Modify: both `tests/support/mod.rs` files and every `connect_pool` caller.
- Modify: `crates/koine-server/src/{serve,dev_loop}.rs`

**Interfaces:**

- Produces: `PoolConfig::new(NonZeroU32, NonZeroU64)`, getters, `Default`, and `connect_pool(url, config)`.
- Consumed by: Tasks 3–4 and all composition/test roots.

- [ ] **Step 1: Add a red pool-options integration test**

```rust
#[tokio::test]
async fn pool_options_are_honored() {
    use std::num::{NonZeroU32, NonZeroU64};
    let (_guard, url) = support::postgres_url().await;
    let config = PoolConfig::new(
        NonZeroU32::new(2).expect("non-zero"),
        NonZeroU64::new(750).expect("non-zero"),
    );
    let pool = connect_pool(&url, config).await.expect("connect");
    assert_eq!(pool.options().get_max_connections(), 2);
    assert_eq!(pool.options().get_acquire_timeout(), Duration::from_millis(750));
}
```

- [ ] **Step 2: Run and observe the signature failure**

Run: `cargo test -p koine-store-postgres --test pool`

Expected: FAIL because `PoolConfig` and the second `connect_pool` argument do not exist.

- [ ] **Step 3: Implement the type-enforced pool config**

```rust
#[derive(Debug, Clone, Copy)]
pub struct PoolConfig {
    max_connections: NonZeroU32,
    acquire_timeout_ms: NonZeroU64,
}

impl PoolConfig {
    #[must_use]
    pub const fn new(max_connections: NonZeroU32, acquire_timeout_ms: NonZeroU64) -> Self {
        Self { max_connections, acquire_timeout_ms }
    }

    #[must_use]
    pub const fn max_connections(self) -> NonZeroU32 { self.max_connections }

    #[must_use]
    pub const fn acquire_timeout(self) -> Duration {
        Duration::from_millis(self.acquire_timeout_ms.get())
    }
}
```

`Default` uses non-zero compile-time constants `16` and `5000`. Build the pool with:

```rust
let pool = PgPoolOptions::new()
    .max_connections(config.max_connections().get())
    .acquire_timeout(config.acquire_timeout())
    .connect(url)
    .await?;
```

- [ ] **Step 4: Add fail-closed server parsing tests**

Extend `ServeConfig` with `pool_config: PoolConfig`. Add valid overrides and four table-driven zero cases:

```rust
for name in [
    "KOINE_MAX_LEASE_TTL_MS",
    "KOINE_IDLE_POLL_MS",
    "KOINE_DB_MAX_CONNECTIONS",
    "KOINE_DB_ACQUIRE_TIMEOUT_MS",
] {
    let vars = HashMap::from([("KOINE_WORKER_TOKEN", "t"), (name, "0")]);
    let err = parse_config(lookup(&vars)).expect_err("zero must fail");
    assert!(err.contains(name));
}
```

Parse duration values through `NonZeroU64` and pool size through `NonZeroU32`; error text is `{name} must be greater than zero`. Keep malformed-value tests for each new variable.

- [ ] **Step 5: Update every explicit caller**

Production server passes parsed config; dev-loop and both test harnesses pass `PoolConfig::default()`. Where signal tests need a URL, add `postgres_url()` returning `(ContainerAsync<Postgres>, String)` and let existing `pg()` call it before constructing the default pool.

- [ ] **Step 6: Run pool/server/workspace compile tests**

```bash
cargo test -p koine-store-postgres --test pool
cargo test -p koine-server
cargo check --workspace --all-targets
```

Expected: pool getters report `2` and `750 ms`; zero cases fail closed.

- [ ] **Step 7: Commit**

```bash
git add crates/koine-store-postgres crates/koine-grpc/tests/support crates/koine-server
git commit -m "feat: configure postgres pool limits"
```

### Task 3: Make presence best-effort in latency

**Files:**

- Modify: `crates/koine-store-postgres/src/presence.rs`
- Modify: `crates/koine-store-postgres/tests/signal.rs`

**Interfaces:**

- Consumes: Task 2's configurable size-one pool.
- Produces: non-blocking acquisition plus `PRESENCE_WRITE_BUDGET = 100 ms`.

- [ ] **Step 1: Add the saturated-pool red regression**

```rust
#[tokio::test]
async fn presence_skips_when_pool_is_saturated() {
    let f = fx_with_pool_size(1).await;
    let held = f.pool.acquire().await.expect("hold only connection");
    let started = std::time::Instant::now();
    tokio::time::timeout(
        Duration::from_millis(250),
        f.presence.seen(&f.worker, Some(&f.queue)),
    )
    .await
    .expect("best-effort presence must not wait for pool timeout");
    assert!(started.elapsed() < Duration::from_millis(250));
    drop(held);
}
```

- [ ] **Step 2: Run and see timeout failure**

Run: `cargo test -p koine-store-postgres --test signal presence_skips_when_pool_is_saturated`

Expected: FAIL because current `.execute(&pool)` waits for the held connection.

- [ ] **Step 3: Implement bounded best-effort behavior**

```rust
const PRESENCE_WRITE_BUDGET: Duration = Duration::from_millis(100);

let Some(mut connection) = self.pool.try_acquire() else {
    return;
};
let _ = tokio::time::timeout(
    PRESENCE_WRITE_BUDGET,
    sqlx::query(PRESENCE_UPSERT)
        .bind(worker_id)
        .bind(last_queue)
        .execute(&mut *connection),
)
.await;
```

Keep the SQL text and `COALESCE` semantics unchanged; give it a private `PRESENCE_UPSERT` constant so the timeout wrapper remains readable.

- [ ] **Step 4: Run all signal/presence tests and commit**

```bash
cargo test -p koine-store-postgres --test signal
git add crates/koine-store-postgres/src/presence.rs crates/koine-store-postgres/tests/signal.rs
git commit -m "fix: bound postgres presence latency"
```

### Task 4: Fan out one dedicated Postgres listener

**Files:**

- Modify: `crates/koine-store-postgres/src/signal.rs`
- Modify: `crates/koine-store-postgres/Cargo.toml`
- Modify: `crates/koine-store-postgres/tests/signal.rs`
- Modify: `crates/koine-grpc/tests/{support/mod.rs,grpc_e2e.rs}`
- Modify: `crates/koine-server/src/serve.rs`

**Interfaces:**

- Produces: `PgSignal::connect(url: &str, notify_pool: PgPool, listener_acquire_timeout: Duration) -> Result<PgSignal, sqlx::Error>`.
- Preserves: `DispatchSignal::{notify,wait}` signatures.

- [ ] **Step 1: Add red fan-out pressure and reconnect tests**

The pressure test creates a size-one operational pool, connects one `PgSignal`, spawns 32 same-queue waits with a five-second timeout, appends one job, and requires every waiter to return within one second. The append must succeed while all waits are pending.

Add `listener_reconnects_after_backend_termination`: identify the dedicated listener in `pg_stat_activity` by its `LISTEN koine_dispatch` query, call `pg_terminate_backend`, then append and require a newly started waiter to wake within the two-second reconnect budget. If SQLx reconnect loses the in-flight notification, append a second job after reconnect; correctness still relies on timeout/recheck.

- [ ] **Step 2: Run and observe pool starvation/signature failure**

Run: `cargo test -p koine-store-postgres --test signal -- --nocapture`

Expected: FAIL under the current one-listener-per-wait implementation.

- [ ] **Step 3: Implement the single listener hub**

Use these fields/constants:

```rust
const NOTIFICATION_BUFFER: usize = 1_024;
const RECONNECT_BACKOFF: Duration = Duration::from_millis(100);

pub struct PgSignal {
    notify_pool: PgPool,
    notifications: tokio::sync::broadcast::Sender<String>,
}
```

`connect` builds a dedicated `PgPoolOptions::new().max_connections(1)` pool
with `listener_acquire_timeout`, connects it to `url`, constructs
`PgListener::connect_with(&listener_pool)`, and awaits
`listen("koine_dispatch")` before creating the broadcast channel and spawning
one task. The listener retains a clone of that dedicated pool for reconnects.
The task loops on `listener.recv()`: successful payloads are sent to the
broadcast channel; errors sleep for `RECONNECT_BACKOFF` and retry.

Implement `wait` as one outer timeout around a receiver loop. Matching payload returns; other queues continue; `Lagged` or `Closed` returns immediately so the fetch loop rechecks dispatch. `notify` remains best-effort on `notify_pool` and never becomes the correctness source.

Enable Tokio features `rt`, `sync`, and `time` in `koine-store-postgres`.

- [ ] **Step 4: Await signal construction at all roots**

Server startup passes `cfg.pool_config.acquire_timeout()` and maps initial
listener failure to `listen koine_dispatch: {e}` before binding gRPC. gRPC e2e
helpers retain the test database URL, pass `PoolConfig::default().acquire_timeout()`,
and await `PgSignal::connect`. Remove every `PgSignal::new` use.

- [ ] **Step 5: Run adapter and real-wire tests**

```bash
cargo test -p koine-store-postgres --test signal
cargo test -p koine-grpc --test grpc_e2e
cargo test -p koine-server
```

Expected: 32 idle waiters do not occupy the operational pool; wakeup/reconnect and gRPC tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/koine-store-postgres crates/koine-grpc/tests crates/koine-server/src/serve.rs
git commit -m "fix: share one postgres notification listener"
```

### Task 5: Document, verify, review, and close resource hardening

**Files:**

- Modify: `.env.example`
- Modify: `docs/architecture/{koine-store-postgres,koine-server,koine-grpc}.md`
- Modify/move: resource item and `phase-2-carryover-hardening.md` to `done/`.

**Interfaces:**

- Consumes: Tasks 1–4.
- Produces: zero open resource gap from closed phases.

- [ ] **Step 1: Document the exact operator contract**

`.env.example` names token, address, TTL, poll, max connections, and acquire timeout with defaults/non-zero constraints. Wiki text states `N + 1` connections, one listener, 100 ms presence budget, timeout fallback, and the phase-3 warning that additional relay/sink concurrency requires capacity review.

- [ ] **Step 2: Run the complete resource gate**

```bash
rg -n "PgSignal::new|PgPool::connect\(" crates
cargo test -p koine-store-postgres
cargo test -p koine-grpc --test grpc_e2e
cargo test -p koine-server
make ci
git diff --check
```

Expected: obsolete constructors/silent pool entrypoints have no matches; all gates pass.

- [ ] **Step 3: Obtain independent spec and quality verdicts**

Reviewer reproduces the size-one pressure and presence saturation tests, checks design §5 and legacy AC4, and records both verdicts. Stop for maintainer review if no independent agent was authorized.

- [ ] **Step 4: Close both records**

Fill exact evidence and `Faithful` statements. Mark legacy AC4 closed with the pool test/wiki evidence, remove its old `→ 2B/3` disposition, then move both items:

```bash
git mv .apptlas/backlog/ongoing/phase-2a-postgres-resource-safety.md .apptlas/backlog/done/
git mv .apptlas/backlog/todo/phase-2-carryover-hardening.md .apptlas/backlog/done/
git add .env.example docs/architecture .apptlas/backlog/done
git commit -m "docs: close postgres resource hardening"
```
