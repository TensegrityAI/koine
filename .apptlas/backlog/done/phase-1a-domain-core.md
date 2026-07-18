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

**Test suite — 51 tests, all green:**

- `cargo test -p koine-domain events` → 3 passed (tag/kind drift +
  envelope round-trip + optional-lineage default; AC1).
- `cargo test -p koine-domain --test state_machine_props` → 3 passed
  (`command_sequences_never_corrupt`, `arbitrary_event_replay_never_panics`,
  `retry_delay_respects_cap`; AC2). Full `koine-domain` unit suite: 30
  passed (ids, queue, retry, events, job — 0 failed).
- `cargo test -p koine-store-memory --test lifecycle` → 8 passed
  (happy path, retryable/non-retryable failure, cancel,
  `worker_crash_is_recovered_by_the_sweep`,
  `late_ack_after_expiry_is_recorded_never_lost`,
  `heartbeats_keep_the_lease_alive`, exhaustion-into-`parked`; AC3/AC4).
- `cargo test -p koine-store-memory` → 9 passed (store append/load,
  version-conflict rejection, phantom-stream regression, dispatch-index
  maintenance; dispatcher priority/FIFO, `not_before` gating, claim+append
  atomicity, extend/expire; AC5).
- `cargo test -p koine-application` → 1 passed (`wrap_events` lineage/
  sequencing test).
- Workspace total: **40 unit + 3 property + 8 ring-2 lifecycle = 51 tests,
  0 failed.**

**Gate:** `make ci` → `✓ all CI checks green` (fmt-check, clippy `-D
warnings`, test, doc, deny, typos, markdownlint — all pass, including this
task's new pages and `docs/formal/lease_protocol.tla`).

**Mid-execution divergences, found and corrected before close (the honest
record):**

- **`Stalled` kind restored per spec** (commits `de84940`, `8ed49a5`): the
  taxonomy task initially dropped the `Stalled` event kind. Caught against
  spec §3 (`JobStalled` is listed under core lifecycle v1, not durable
  execution), the phase-1a plan was corrected first, then the enum fixed —
  bringing the taxonomy back to the full 19 kinds this record and the
  brief cite. `Job::apply` treats it as an informational no-op while
  `leased`/`running`, matching its "produced by phase 2's heartbeat
  mechanics" status.
- **`wrap_events` signature restored** (commit `73c18a0`): an intermediate
  version of the application-layer envelope factory drifted from the
  signature every planned call site (tasks 9–12) needed. Restored to the
  planned `wrap_events(ids, clock, stream, base_version, correlation_id,
  causation_id, traceparent, events)` shape before any use case was built
  against it, so `enqueue`, `worker_ack`, `cancel`, and `sweep` all call it
  identically.
- **Phantom-stream bug fixed** (commit `b468996`): the first
  `InMemoryEventStore::append_locked` computed the stream's current length
  via `HashMap::entry(...).or_default()`, which materialized an empty
  stream as a side effect even when the append was then rejected for a
  version conflict — a rejected append against a never-seen `JobId` left a
  phantom empty stream that `load` would then find. Fixed by computing the
  current length from `.get()` (no insertion) and running all validation
  before any map mutation, so failures are side-effect-free. Regression
  test: `failed_append_leaves_no_phantom_stream`.
- Final whole-branch review: M1 fold-rejected-append side effect found and
  fixed (see fix commits); M2-M5 + polish applied; full gate green
  post-fix.

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
- Spec §3 draws `scheduled` as its own resting state; modeled as
  `Pending { not_before: Some(_) }` — same semantics, one fewer state
  (disposition: recorded here, final review 2026-07-18).
