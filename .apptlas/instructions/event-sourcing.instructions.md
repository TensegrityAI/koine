# Instructions: Event sourcing

**Applies to:** `crates/koine-domain/**`, `crates/koine-application/**`,
`crates/koine-store-*/**`

- **The log is append-only truth** (ADR 0004). Recorded events are never
  mutated or deleted — corrections are new events (`JobRepaired`, conflict
  events). Any code path that updates or removes an event row is Critical.
- **State is a fold over events.** Aggregates rebuild from their event
  sequence; anything not derivable from the log plus declared ephemera
  (heartbeats, progress %) is a design error.
- **Every event carries its lineage**: `correlation_id`, `causation_id`, W3C
  trace context (design spec §3). No event type ships without them.
- **Appends use optimistic concurrency** (expected version); conflicts
  surface as typed errors, never as silent retries that reorder history.
- **Dispatch-critical projection updates ride the append transaction**; all
  other projections go through the transactional outbox — event and outbox
  row in the same transaction, relay with persisted positions (ADR 0006).
  A dual-write (save then publish separately) is Critical.
- **Projections are rebuildable**: every projection must replay from event
  zero to an identical state; ring-3 tests prove it.
- **State machines are proptest-covered** (ring 1): no event sequence may
  reach an illegal state (testing-policy).
