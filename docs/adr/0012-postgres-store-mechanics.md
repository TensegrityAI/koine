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
