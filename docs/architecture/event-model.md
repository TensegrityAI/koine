# Event model

The taxonomy reference for `koine_domain::JobEvent` (`crates/koine-domain/src/events.rs`)
and its envelope. This is the wire/storage contract (ADR 0010) — kind strings
are never renamed.

## What it is

One Rust enum, `JobEvent`, holds all 19 event kinds a job can ever emit.
11 are active in phase 1A (rings 1–2); 8 are reserved for phase 5's durable
execution and are defined — with real (if minimal) transition semantics in
`Job::apply` — but not yet produced by any use case, per ADR 0004's decision
to design the complete taxonomy from day one and implement it in phases.

## Taxonomy

| Kind string | Status | Emitted by |
| --- | --- | --- |
| `enqueued` | v1 — active | `EnqueueJob` use case (always version 1 of a stream) |
| `leased` | v1 — active | `Dispatcher::lease_next` (via `Job::lease`) |
| `started` | v1 — active | `WorkerAck::start` (via `Job::start`) |
| `succeeded` | v1 — active | `WorkerAck::succeed` (via `Job::succeed`) |
| `failed` | v1 — active | `WorkerAck::fail` (via `Job::fail`), paired with a retry decision |
| `lease_expired` | v1 — active | `SweepExpiredLeases` (via `Job::expire_lease`), paired with a retry decision |
| `retry_scheduled` | v1 — active | The retry decision from `Job::fail`/`Job::expire_lease` when attempts remain |
| `parked` | v1 — active | The retry decision when attempts are exhausted, or a non-retryable `failed` |
| `cancelled` | v1 — active | `CancelJob` use case (via `Job::cancel`), legal until terminal |
| `late_ack_conflict` | v1 — active | `WorkerAck` when a worker's lease no longer matches (spec §3: never discarded) |
| `stalled` | v1 — active, not yet produced | Reserved for phase 2's heartbeat mechanics (stall-threshold crossing); `Job::apply` already accepts it as an informational no-op while `Leased`/`Running` |
| `checkpoint_recorded` | reserved — phase 5 | Journaled side-effect result while `Running` |
| `signal_received` | reserved — phase 5 | External signal delivered to a non-terminal job |
| `approval_requested` | reserved — phase 5 | Raised from `Running`, moves the job to `AwaitingApproval` |
| `approval_granted` | reserved — phase 5 | Resolves `AwaitingApproval` back to `Pending` |
| `approval_denied` | reserved — phase 5 | Resolves `AwaitingApproval` to `Parked { reason: ApprovalDenied }` |
| `suspended` | reserved — phase 5 | From `Pending`/`Running` to `Suspended` |
| `resumed` | reserved — phase 5 | From `Suspended` back to `Pending` |
| `repaired` | reserved — phase 5 | From `Parked`; resets `attempt` to 0, optionally replaces `payload`, preserves all prior history |

The kind string is the serde tag (`#[serde(tag = "type", rename_all =
"snake_case")]`) and is identical to `JobEvent::kind()` — a drift test
(`events::tests::serde_tag_matches_kind_for_every_variant`) asserts this for
every variant on every CI run.

## State diagram (rings 1–2, spec §3)

```text
                 lease           start          succeed
   pending ─────────────▶ leased ─────▶ running ─────────▶ succeeded
      ▲  ▲                  │              │  │
      │  │  retry_scheduled │ lease_expired│  │ failed (retryable)
      │  └──────────────────┴──────────────┘  │
      │                                        ▼
      │                                    failed (fatal) / exhausted
      │                                        │
      │                                        ▼
      │                                     parked (repairable; phase-5 `repaired` re-enters here)
      │
      └── cancelled reachable from pending/leased/running/parked (terminal)

  late_ack_conflict: legal in every state, changes none (pure record)
  stalled: legal while leased/running, changes none (informational)
```

Phase-5 states (`suspended`, `awaiting_approval`) and their transitions
(`suspended`/`resumed`/`approval_requested`/`approval_granted`/
`approval_denied`/`checkpoint_recorded`/`signal_received`) are defined in
`Job::apply` but reachable only once phase 5 use cases emit them.

## Envelope (`EventEnvelope`)

Every event is stored wrapped in one envelope shape:

| Field | Type | Meaning |
| --- | --- | --- |
| `event_id` | `EventId` (UUIDv7) | This event's identity |
| `stream_id` | `JobId` | The job (= stream) this belongs to |
| `version` | `u64` | 1-based position in the stream; the optimistic-concurrency token |
| `recorded_at` | `DateTime<Utc>` | Broker-side record time |
| `correlation_id` | `CorrelationId` | Correlates every event of one logical operation |
| `causation_id` | `Option<EventId>` | The event that caused this one |
| `traceparent` | `Option<String>` | W3C trace context, carried end to end |
| `schema_version` | `u16` | Envelope-shape version (`SCHEMA_VERSION = 1`) |
| `event` | `JobEvent` | The event itself |

## Lineage rules

- **Correlation is carried from `enqueued`.** `EnqueueJob` mints a fresh
  `correlation_id` only if the caller didn't supply one; every later
  append for that stream (`worker_ack`, `cancel`, `sweep`, the dispatcher's
  `lease_next`) reads it back off the stream's first envelope and reuses it.
- **Causation is the previous event.** Every use case that appends to an
  existing stream sets `causation_id` to the `event_id` of the stream's last
  envelope before the append. When one command produces a multi-event batch
  (`failed` + `retry_scheduled`, or `lease_expired` + `retry_scheduled`), the
  whole batch shares that same causation — it is not chained event-to-event
  within the batch, since both were caused by the same triggering fact.
- **`traceparent` also rides the first envelope** and is carried into every
  later append the same way as `correlation_id`.

## Additive-evolution rules (ADR 0010)

- New fields on an existing variant get `#[serde(default)]` so old records
  without them still deserialize (`envelope_deserializes_without_optional_lineage`
  is the regression test for the envelope's optional fields).
- Renames and removals are not allowed on a shipped kind — a changed
  meaning is a **new** event kind instead.
- `schema_version` bumps only when the *envelope* shape changes, not when a
  new event kind or optional field is added.
- Ids are UUIDv7 newtypes generated only behind the application
  `IdGenerator` port, keeping `koine-domain` free of clocks and randomness.
