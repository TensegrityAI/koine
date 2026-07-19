# 0015 — Worker presence as ephemeral state

- **Status:** accepted
- **Date:** 2026-07-19
- **Context:** Spec §2 lists `WorkerRegistration` among domain aggregates;
  the phase-1 disposition deferred it to phase 2 "where workers first
  connect." But spec §3's event taxonomy defines zero worker events, and
  the spec's own doctrine makes high-frequency liveness data ephemeral
  (heartbeats). The two spec signals conflict; this ADR resolves it.
- **Decision:** worker presence is **ephemeral infrastructure state**, like
  lease deadlines (ADR 0011-c): a `workers` table (`worker_id` PK,
  `first_seen`, `last_seen`, `last_queue`) upserted on every authenticated
  data-plane call. No domain events, no aggregate, no stream. It feeds
  phase-3 dashboards and operational queries (`SELECT … WHERE last_seen >
  now() - interval '1 minute'`).
- **Consequences:** no audit history of worker fleet churn (revisit if a
  real consumer appears — that would justify event-sourcing worker
  lifecycle and generifying `EventStore`, the recorded 1A trigger);
  presence survives restarts as stale rows — readers filter by `last_seen`;
  the spec §2 aggregate list is superseded on this point by this ADR
  (spec-fidelity: divergence with disposition, recorded here).
- **Alternatives considered:** event-sourced WorkerRegistration aggregate
  (second aggregate would force EventStore generification now, for data
  nobody consumes yet); in-memory-only presence (lost on restart, invisible
  to operators querying the DB); no presence at all (phase-3 dashboards
  would have nothing to show for the fleet).
