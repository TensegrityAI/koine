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
