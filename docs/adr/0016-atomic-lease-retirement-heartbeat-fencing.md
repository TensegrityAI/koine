# 0016 — Atomic lease retirement and heartbeat fencing

- **Status:** accepted
- **Date:** 2026-07-21
- **Context:** ADR-0008 defines at-least-once delivery with ephemeral heartbeat
  extensions and event-sourced lease expiry. ADR-0011(c) assigns expiry
  discovery to `Dispatcher::expired` but leaves the sweep use case to load the
  stream and append `LeaseExpired` later. A heartbeat can extend the projection
  deadline between those operations without moving the event-stream version,
  so optimistic concurrency does not fence an expiry decision based on the old
  deadline. Koiné must not revoke a heartbeat it already accepted.
- **Decision:** Replace split expiry discovery with an atomic dispatcher
  operation that selects one currently expired lease, locks and revalidates the
  active grant, derives expiry/retry events through the domain aggregate,
  appends them, and updates the dispatch projection in one transaction or
  in-memory critical section. `extend_lease` and retirement serialize on the
  same lease row/state: heartbeat-first preserves the extended lease;
  retirement-first makes the heartbeat return `false`. Remove the public
  `expired(now) -> Vec<JobId>` contract; the sweep loops the atomic operation.
  Extend the TLA+ protocol with time, deadline, lease identity, and heartbeat,
  and verify the fence plus conditional recovery liveness.
- **Consequences:** Accepted heartbeats cannot be invalidated by a stale sweep;
  concurrent sweepers remain safe through row locking and `SKIP LOCKED`; memory
  and Postgres adapters share one stronger contract. The dispatcher adapter
  carries more domain/event-store responsibility, and one transaction is used
  per retired lease. The internal application port changes, but events and the
  `koine.v1` wire contract do not. On acceptance, this ADR supersedes only
  ADR-0011 decision (c); decisions (a) and (b) remain accepted. ADR-0008 is
  refined, not superseded.
- **Alternatives considered:** recheck after `expired` (still not atomic with
  heartbeat); append `LeaseExtended` on every heartbeat (log amplification and
  contrary to accepted ephemera); add only a lease-generation predicate to the
  later append (the event store does not own ephemeral deadlines); merge
  `Dispatcher` into `EventStore` (unnecessarily broad god-port).
