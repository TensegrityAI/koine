# `koine-domain`

## What it does

The pure event-sourced core: the `Job` aggregate, the `JobEvent` taxonomy,
and `RetryPolicy`. State is never mutated directly — it is always the fold
of a job's recorded events (ADR 0004). This crate has no async runtime, no
clock, and no randomness; every fact it needs (time, ids, jitter seed) is
handed in by the caller.

## How it is built

- **`Job` (`src/job.rs`)** — `Job::from_events(&[EventEnvelope])` rebuilds
  an aggregate by folding a stream from its first event (which must be
  `Enqueued`, at version 1) through `Job::apply`. `apply` is the transition
  table: one `match` on `(current JobState, &JobEvent)` that returns the next
  state or `DomainError::IllegalTransition`, written as a single function on
  purpose so the whole state machine is auditable in one place.
- **Commands vs. events** — `Job` exposes command methods (`lease`, `start`,
  `succeed`, `fail`, `expire_lease`, `cancel`, `late_ack`) that validate
  against the current state and *return* the event(s) to append; they do not
  mutate `self`. `fail` and `expire_lease` each return two events (the
  outcome plus the retry decision) so both land in one atomic append.
  `late_ack` is always legal — a stale ack is a pure record, never rejected
  (spec §3: information is never discarded).
- **States (`JobState`)** — `Pending { not_before }`, `Leased`, `Running`,
  `Succeeded`, `Parked { reason }`, `Cancelled`, plus `Suspended` and
  `AwaitingApproval { key }` reserved for phase 5. Terminal states
  (`Succeeded`, `Cancelled`) absorb every event except `LateAckConflict`.
- **Retry (`src/retry.rs`)** — `RetryPolicy::decide(attempts_completed, seed)`
  is a pure function: exponential backoff with full jitter, uniform in
  `[0, min(base * 2^(n-1), cap)]`, driven by a dependency-free `SplitMix64`
  step so equal `(policy, attempt, seed)` always yields equal output. The
  `seed` comes from the application's `IdGenerator` port — the domain itself
  never touches an RNG.
- **Verification** — `crates/koine-domain/tests/state_machine_props.rs` is a
  proptest suite with three properties: arbitrary command sequences never
  corrupt the aggregate (`attempt` monotonic, `version` counts applied
  events, terminal states absorb), arbitrary event replay never panics
  (every event either applies with `version + 1` or is rejected untouched),
  and retry delay never exceeds its cap.

## Why

- ADR 0004 — the event log is the single source of truth; `Job` is that
  decision made concrete as a fold.
- ADR 0010 — event encoding and identity: `JobEvent` is one internally-tagged
  enum (`#[serde(tag = "type", rename_all = "snake_case")]`), the tag string
  is `JobEvent::kind()`, ids are UUIDv7 newtypes generated only behind the
  application's `IdGenerator` port, and evolution is additive-only.

## Boundaries

- Depends on nothing internal — this is the innermost layer of the hexagon
  (ADR 0003).
- Allowed dependencies are pure-data only: `serde`, `serde_json`, `uuid`,
  `chrono`, `thiserror` (ADR 0010). No I/O, no clock, no randomness — those
  are `koine-application` ports (`Clock`, `IdGenerator`).
- Everything above it (`koine-application`, adapters) depends on this crate;
  it depends on nothing above it.
