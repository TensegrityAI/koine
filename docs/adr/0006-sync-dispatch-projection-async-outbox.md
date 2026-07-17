# 0006 — Sync dispatch projection, async outbox

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** Event sourcing usually dies on throughput; Koiné splits consistency by criticality across two projection tiers instead of picking one strategy for everything. Among the known kineticrs gaps Koiné fixes at birth is the absence of a transactional outbox, which left kineticrs with a dual-write problem.

- **Decision:** The `dispatch_queue` table — what a worker fetches from — is updated in the same transaction as the event append; workers fetch via `SELECT … FOR UPDATE SKIP LOCKED`, the most battle-tested Postgres job-queue pattern. Every other projection (history, metrics, dashboard views) is updated asynchronously via a transactional outbox: the event and an outbox row are written in one transaction, and a relay with persistent positions delivers them; these projections are rebuildable from the log at any time.

- **Consequences:**
  - Easier: dispatch is strongly consistent exactly where it matters — a worker can never fetch a job whose event append didn't commit — and the dual-write gap kineticrs had is closed; rebuildable async projections mean history/metrics/dashboard schemas can evolve or be repaired by replay.
  - Harder: folding `dispatch_queue` maintenance into the same transaction as the event append adds work to the hot-path transaction, and Postgres hot-path throughput under event sourcing is a risk called out for benchmarking in phase 2; async projections are only eventually consistent, so history/metrics/dashboard views can lag briefly behind the true event log.
  - Gave up: a simpler single-tier projection model where every projection is treated the same way.

- **Alternatives considered:**
  - All-async projections, including dispatch — rejected: dispatch lag (a worker fetching a job before, or without ever seeing, its committed event) is unacceptable on the one path that must always be consistent.
  - All-sync projections — rejected: puts every projection inside the hot-path transaction, creating a throughput ceiling.
