# Koiné Atomic Lease Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make heartbeat renewal and lease retirement one serializable protocol so an accepted heartbeat can never be revoked by a stale sweep.

**Architecture:** Replace `Dispatcher::expired` with an adapter-owned `retire_next_expired_lease` composite. The memory adapter performs the whole transition under its store mutex; Postgres performs selection, fold, domain decision, append, and projection update in one row-locking transaction. Extend the TLA+ model before Rust implementation so heartbeat/deadline interleavings define the executable contract.

**Tech Stack:** Rust 1.95, Tokio, SQLx/Postgres, testcontainers, TLA+/TLC, Markdown governance.

## Global Constraints

- Implements approved ADR-0016 and design §§3–4, 9–10.
- One `koine-server`, one relay, one Postgres database; no HA coordination.
- No event-kind, migration, or `koine.v1` wire-shape change.
- Heartbeat remains ephemeral; do not add `LeaseExtended` events.
- Domain `Job::expire_lease` remains the only source of expiry/retry events.
- TDD is mandatory: observe each red result before implementation.
- Phase 2B remains blocked throughout this plan.

---

## File map

- `docs/formal/lease_protocol.tla` / `.cfg`: heartbeat-aware protocol and bounds.
- `docs/formal/README.md`: checked properties, conditional liveness, mutation evidence.
- `crates/koine-application/src/ports.rs`: strengthened dispatcher contract/error.
- `crates/koine-application/src/use_cases/sweep.rs`: thin loop over atomic retirement.
- `crates/koine-store-memory/src/dispatcher.rs`: mutex-atomic reference adapter.
- `crates/koine-store-postgres/src/dispatcher.rs`: transaction-atomic production adapter.
- `crates/*/tests` and gRPC/server call sites: contract regressions and constructor updates.
- `docs/architecture/{koine-application,koine-store-memory,koine-store-postgres}.md`: live architecture.
- `.apptlas/backlog/{todo,ongoing,done}/phase-2a-atomic-lease-retirement.md`: lifecycle evidence.

### Task 1: Open the ready hardening item

**Files:**

- Create: `.apptlas/backlog/todo/phase-2a-atomic-lease-retirement.md`
- Move when ready: `.apptlas/backlog/ongoing/phase-2a-atomic-lease-retirement.md`

**Interfaces:**

- Consumes: approved hardening design and ADR-0016.
- Produces: the acceptance/evidence owner for Tasks 2–5.

- [ ] **Step 1: Create the item from the repository template**

Use this complete acceptance core:

```markdown
# Make lease retirement atomic with heartbeat renewal

- **State:** todo
- **Origin:** phase-2A zero-debt hardening design
- **Epic:** ../epics/phase-2-data-plane.md

## Traceability

- **Implements:** hardening design §§3–4, 9–10; ADR-0016; atomic-lease plan Tasks 1–5.

## Acceptance criteria

- [ ] AC1: the TLA+ model includes time, deadline, lease identity, bounded heartbeats, and `HeartbeatExpiryFence`; TLC passes and the documented stale-expiry mutation fails that invariant — *verify:* `make tla` plus mutation evidence in `docs/formal/README.md`.
- [ ] AC2: `Dispatcher` exposes atomic `retire_next_expired_lease`; no `expired` list contract remains — *verify:* `rg -n "fn expired|\.expired\(" crates` returns no matches and ring 2 is green.
- [ ] AC3: heartbeat-first preserves the renewal, retirement-first rejects heartbeat, and two sweepers retire one grant once in memory and Postgres — *verify:* named ring-2/ring-3 regressions.
- [ ] AC4: expiry/retry events still come from `Job::expire_lease`, retain lineage, and late ACK remains a conflict — *verify:* mirrored lifecycle suites and gRPC e2e.
- [ ] AC5: architecture docs and every sweep call site describe/use the atomic contract — *verify:* docs review, `make ci`, `make tla`.

## Dependencies

- Approved ADR-0016 and hardening design; both accepted 2026-07-21.

## Evidence (filled at close)

## Spec-fidelity statement (filled at close)
```

- [ ] **Step 2: Check Definition of Ready and move the item**

Run:

```bash
git diff --check
git mv .apptlas/backlog/todo/phase-2a-atomic-lease-retirement.md .apptlas/backlog/ongoing/
```

Expected: the item has observable ACs, traceability, dependencies, and one-review-cycle scope.

- [ ] **Step 3: Commit the ready item**

```bash
git add .apptlas/backlog/ongoing/phase-2a-atomic-lease-retirement.md
git commit -m "docs: open atomic lease retirement hardening"
```

### Task 2: Model heartbeat fencing before Rust

**Files:**

- Modify: `docs/formal/lease_protocol.tla`
- Modify: `docs/formal/lease_protocol.cfg`
- Modify: `docs/formal/README.md`

**Interfaces:**

- Consumes: ADR-0016's heartbeat-first/retirement-first serialization.
- Produces: `Heartbeat`, `Tick`, `HeartbeatExpiryFence`, and conditional `EventuallySettled` semantics used by Tasks 3–4.

- [ ] **Step 1: Add the deadline model with an intentionally stale expiry guard**

Add constants and state:

```tla
CONSTANTS Workers, MaxAttempts, MaxLeases, MaxConflicts, MaxHeartbeats, LeaseTtl

VARIABLES
    state, attempt, activeLease, issued, conflicts,
    now, deadline, heartbeats,
    lastAckLease, lastAckActiveLease, lastFailRetryable, lastFailParked,
    lastHeartbeatLease, lastHeartbeatDeadline, lastExpiredLease, lastExpiryNow
```

Initialize the new variables to zero, set `deadline' = now + LeaseTtl` in `Lease`, and add:

```tla
Tick ==
    /\ state \in {"leased", "running"}
    /\ now < deadline
    /\ now' = now + 1
    /\ UNCHANGED <<state, attempt, activeLease, issued, conflicts, deadline,
                    heartbeats, lastAckLease, lastAckActiveLease,
                    lastFailRetryable, lastFailParked, lastHeartbeatLease,
                    lastHeartbeatDeadline, lastExpiredLease, lastExpiryNow>>

Heartbeat ==
    /\ state \in {"leased", "running"}
    /\ now < deadline
    /\ heartbeats < MaxHeartbeats
    /\ deadline' = now + LeaseTtl
    /\ heartbeats' = heartbeats + 1
    /\ lastHeartbeatLease' = activeLease
    /\ lastHeartbeatDeadline' = now + LeaseTtl
    /\ UNCHANGED <<state, attempt, activeLease, issued, conflicts, now,
                    lastAckLease, lastAckActiveLease, lastFailRetryable,
                    lastFailParked, lastExpiredLease, lastExpiryNow>>

HeartbeatExpiryFence ==
    lastExpiredLease = NoLease
    \/ lastExpiredLease # lastHeartbeatLease
    \/ lastExpiryNow >= lastHeartbeatDeadline
```

For the red probe only, give `Expire` the deliberately stale/early guard
`now >= deadline - LeaseTtl` rather than the current-deadline guard, while
recording `lastExpiredLease' = activeLease` and `lastExpiryNow' = now`. Add
`Heartbeat`, `Tick`, and their fairness to `Next`/`Spec`; add the invariant to
the cfg with:

```tla
    MaxHeartbeats = 2
    LeaseTtl = 2
```

- [ ] **Step 2: Run TLC and observe the stale-expiry counterexample**

Run: `make tla`

Expected: FAIL with `Invariant HeartbeatExpiryFence is violated`; record the shortest trace depth in the ongoing item.

- [ ] **Step 3: Fence expiry on the current deadline**

Use this guard and state update:

```tla
Expire ==
    /\ state \in {"leased", "running"}
    /\ now >= deadline
    /\ attempt' = attempt + 1
    /\ activeLease' = NoLease
    /\ deadline' = 0
    /\ state' = IF attempt + 1 >= MaxAttempts THEN "parked" ELSE "pending"
    /\ lastExpiredLease' = activeLease
    /\ lastExpiryNow' = now
    /\ UNCHANGED <<issued, conflicts, now, heartbeats, lastAckLease,
                    lastAckActiveLease, lastFailRetryable, lastFailParked,
                    lastHeartbeatLease, lastHeartbeatDeadline>>
```

Every action that ends a live lease resets `deadline` to zero; every action
not changing time/deadline lists them in `UNCHANGED`. Extend `TypeOK` with:

```tla
    /\ now \in Nat
    /\ deadline \in Nat
    /\ heartbeats \in 0..MaxHeartbeats
    /\ now <= (issued + heartbeats) * LeaseTtl
    /\ deadline <= (issued + heartbeats) * LeaseTtl
    /\ lastHeartbeatLease \in 0..MaxLeases
    /\ lastHeartbeatDeadline \in Nat
    /\ lastExpiredLease \in 0..MaxLeases
    /\ lastExpiryNow \in Nat
```

Define liveness as conditional on the finite `MaxHeartbeats` environment
bound. Keep `WF_vars(Lease)`, and add weak fairness for `Tick` and `Expire`.

- [ ] **Step 4: Run the green model and document the bound**

Run: `make tla`

Expected: `Model checking completed. No error has been found.` Update `docs/formal/README.md` with the new action, invariant, state counts, finite-heartbeat bound, and mutation-probe failure.

- [ ] **Step 5: Commit the formal contract**

```bash
git add docs/formal/lease_protocol.tla docs/formal/lease_protocol.cfg docs/formal/README.md .apptlas/backlog/ongoing/phase-2a-atomic-lease-retirement.md
git commit -m "model: verify heartbeat expiry fencing"
```

### Task 3: Strengthen the port and in-memory reference adapter

**Files:**

- Modify: `crates/koine-application/src/ports.rs`
- Modify: `crates/koine-application/src/use_cases/sweep.rs`
- Modify: `crates/koine-store-memory/src/dispatcher.rs`
- Modify: `crates/koine-store-memory/tests/lifecycle.rs`
- Modify: `crates/koine-grpc/tests/{wire.rs,fetch_idle_disconnect.rs}`

**Interfaces:**

- Consumes: the modeled atomic action from Task 2.
- Produces: `Dispatcher::retire_next_expired_lease() -> Result<Option<JobId>, DispatchError>` and `SweepExpiredLeases<'a, D> { dispatcher: &'a D }`.

- [ ] **Step 1: Write ring-2 red tests against the new contract**

Add tests named:

```rust
#[tokio::test]
async fn heartbeat_first_fences_retirement() {
    let f = fixture();
    let job = enqueue(&f, 0, None).await;
    let leased = f.dispatcher.lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await.expect("claim").expect("job");
    f.clock.advance(Duration::from_secs(20));
    assert!(f.dispatcher.extend_lease(leased.lease, Duration::from_secs(30)).await.expect("hb"));
    f.clock.advance(Duration::from_secs(11));
    assert_eq!(f.dispatcher.retire_next_expired_lease().await.expect("retire"), None);
    assert_eq!(f.store.load(job).await.expect("load").len(), 2);
}

#[tokio::test]
async fn retirement_first_rejects_heartbeat_and_happens_once() {
    let f = fixture();
    enqueue(&f, 0, None).await;
    let leased = f.dispatcher.lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await.expect("claim").expect("job");
    f.clock.advance(Duration::from_secs(31));
    assert_eq!(f.dispatcher.retire_next_expired_lease().await.expect("retire"), Some(leased.job_id));
    assert_eq!(f.dispatcher.retire_next_expired_lease().await.expect("retire twice"), None);
    assert!(!f.dispatcher.extend_lease(leased.lease, Duration::from_secs(30)).await.expect("hb"));
}
```

- [ ] **Step 2: Run the focused tests and observe compile failure**

Run: `cargo test -p koine-store-memory dispatcher -- --nocapture`

Expected: FAIL because `retire_next_expired_lease` is not yet a `Dispatcher` method.

- [ ] **Step 3: Replace the split port and thin the sweep**

Add `DispatchError::Domain(#[from] DomainError)` and replace `expired` with:

```rust
/// Atomically retires one currently expired lease through the domain and
/// records its events, or returns `None` when no expired grant is available.
fn retire_next_expired_lease(
    &self,
) -> impl Future<Output = Result<Option<JobId>, DispatchError>> + Send;
```

Reduce the use case to:

```rust
pub struct SweepExpiredLeases<'a, D> {
    pub dispatcher: &'a D,
}

impl<D: Dispatcher> SweepExpiredLeases<'_, D> {
    pub async fn execute(&self) -> Result<u32, SweepError> {
        let mut swept = 0;
        while self.dispatcher.retire_next_expired_lease().await?.is_some() {
            swept += 1;
        }
        Ok(swept)
    }
}
```

`SweepError` retains only the transparent `Dispatch` variant.

- [ ] **Step 4: Implement memory retirement under one lock**

Inside `InMemoryDispatcher`, read `now`, choose the lowest expired job ID, fold, call `job.expire_lease(now, self.ids.jitter_seed())`, preserve lineage through `wrap_events`, and call `append_locked` without releasing the lock. Move `extend_lease`'s `clock.now()` and deadline calculation into the same `store.locked` closure.

Use this public implementation shape:

```rust
fn retire_next_expired_lease(
    &self,
) -> impl Future<Output = Result<Option<JobId>, DispatchError>> + Send {
    let result = self.retire_one();
    async move { result }
}
```

- [ ] **Step 5: Update generic wrappers and sweep construction**

`CountingDispatcher` forwards `retire_next_expired_lease`; every memory lifecycle/wire sweeper becomes `SweepExpiredLeases { dispatcher: &... }`. Remove obsolete store/ids/clock generic parameters and imports.

- [ ] **Step 6: Run ring 2 and gRPC memory tests**

Run:

```bash
cargo test -p koine-store-memory
cargo test -p koine-grpc --test wire --test fetch_idle_disconnect
```

Expected: all tests pass; heartbeat-first leaves only `enqueued,leased`, retirement-first records one expiry decision.

- [ ] **Step 7: Commit the reference contract**

```bash
git add crates/koine-application crates/koine-store-memory crates/koine-grpc/tests
git commit -m "fix: make memory lease retirement atomic"
```

### Task 4: Implement Postgres serialization and race regressions

**Files:**

- Modify: `crates/koine-store-postgres/src/dispatcher.rs`
- Modify: `crates/koine-store-postgres/tests/{dispatcher,lifecycle}.rs`
- Modify: `crates/koine-grpc/tests/grpc_e2e.rs`
- Modify: `crates/koine-server/src/{serve,dev_loop}.rs`

**Interfaces:**

- Consumes: Task 3's exact dispatcher/sweep signatures.
- Produces: Postgres row-lock serialization implementing ADR-0016.

- [ ] **Step 1: Add real-Postgres red regressions**

Add three named tests to `tests/dispatcher.rs`: `heartbeat_first_fences_retirement`, `retirement_first_rejects_heartbeat`, and `concurrent_retirement_records_one_expiry`. The concurrent assertion is:

```rust
let (left, right) = tokio::join!(
    f.dispatcher.retire_next_expired_lease(),
    f.dispatcher.retire_next_expired_lease(),
);
let retired = [left.expect("left"), right.expect("right")]
    .into_iter().flatten().collect::<Vec<_>>();
assert_eq!(retired, vec![claimed.job_id]);
let kinds = f.store.load(claimed.job_id).await.expect("load")
    .into_iter().map(|e| e.event.kind()).collect::<Vec<_>>();
assert_eq!(kinds.iter().filter(|kind| **kind == "lease_expired").count(), 1);
```

Also add the controlled-lock regression: hold `SELECT ... FOR UPDATE` on the dispatch row, start heartbeat before the old deadline, advance the injected clock past it, assert retirement returns `None` because the row is locked, release the lock, and assert heartbeat succeeds. This recreates the stale-clock interleaving that the old split protocol could mishandle.

- [ ] **Step 2: Run ring 3 and observe failure**

Run: `cargo test -p koine-store-postgres --test dispatcher -- --nocapture`

Expected: FAIL until Postgres implements the new method.

- [ ] **Step 3: Implement one transaction per retirement**

Add `retire_one` that begins a transaction before reading `Clock`, selects one expired row using:

```sql
SELECT job_id
FROM event_store.dispatch_queue
WHERE lease_id IS NOT NULL AND lease_expires_at <= $1
ORDER BY lease_expires_at, job_id
LIMIT 1
FOR UPDATE SKIP LOCKED
```

Within that transaction call `load_in_tx`, `Job::from_events`, `Job::expire_lease`, `lineage_of`, `wrap_events`, and `append_in_tx`, then commit and return the ID. Do not catch `IllegalTransition` or version conflicts: the lock makes either a real invariant/backend error.

Change `extend_lease` to begin/acquire a transaction before reading `Clock`; perform the predicate update and commit. PostgreSQL re-evaluates the `WHERE lease_id = $2 AND lease_expires_at > $3` predicate after a conflicting row update, producing the required retirement-first `false` outcome.

- [ ] **Step 4: Update all production/e2e sweep constructors**

Use only:

```rust
let sweep = SweepExpiredLeases { dispatcher: &dispatcher };
```

Remove now-unused sweep-local stores, IDs, clocks, imports, and generics from server, dev-loop, and gRPC tests.

- [ ] **Step 5: Run Postgres, gRPC, and server tests**

Run:

```bash
cargo test -p koine-store-postgres --test dispatcher --test lifecycle
cargo test -p koine-grpc --test grpc_e2e
cargo test -p koine-server
```

Expected: all pass; concurrent retirement writes exactly one `lease_expired` event.

- [ ] **Step 6: Commit the production fence**

```bash
git add crates/koine-store-postgres crates/koine-grpc/tests/grpc_e2e.rs crates/koine-server
git commit -m "fix: fence postgres heartbeat and lease expiry"
```

### Task 5: Document, verify, review, and close the slice

**Files:**

- Modify: `docs/architecture/{koine-application,koine-store-memory,koine-store-postgres}.md`
- Modify: `.apptlas/backlog/ongoing/phase-2a-atomic-lease-retirement.md`
- Move: item to `.apptlas/backlog/done/`

**Interfaces:**

- Consumes: all previous tasks.
- Produces: independently reviewable closure evidence required before the resource plan.

- [ ] **Step 1: Reconcile architecture text**

Replace every description of `expired` listing with the atomic operation, link ADR-0016, state the two lock outcomes, and document conditional formal liveness. Keep event and wire contracts explicitly unchanged.

- [ ] **Step 2: Run the complete slice gate**

```bash
rg -n "fn expired|\.expired\(" crates
make tla
make ci
git diff --check
```

Expected: `rg` has no matches; TLC and CI are green.

- [ ] **Step 3: Obtain both independent review verdicts**

The reviewer reads ADR-0016/design §§3–4, reproduces the controlled race and TLC run, and records:

```markdown
- Spec compliance: ✅ Faithful to ADR-0016 and hardening design §§3–4.
- Quality: Approved — no Critical, Important, or unrecorded Minor findings.
```

If inline execution has no independent agent authorization, stop here and request maintainer review; do not self-certify DoD item 8.

- [ ] **Step 4: Fill evidence and close**

Record exact commands, test counts, TLC state counts, red-first evidence, review verdicts, and `Faithful`. Then:

```bash
git mv .apptlas/backlog/ongoing/phase-2a-atomic-lease-retirement.md .apptlas/backlog/done/
git add docs/architecture .apptlas/backlog/done/phase-2a-atomic-lease-retirement.md
git commit -m "docs: close atomic lease retirement hardening"
```
