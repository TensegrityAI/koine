# 0004 — Event log as single source of truth

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** Koiné's core thesis is that the history of every job is the source of truth, not a byproduct; from that commitment follow total traceability, repair & resume (durable execution), and agent-native operation. Every job-lifecycle transition is meant to be an immutable event in the log. Durable-execution needs (checkpoints, signals, approvals) must be designed for from day one even though they are only implemented in phase 5, so that later phases require no architectural change.

- **Decision:** All job, queue, and worker state derives from an append-only event log. The v1 schema reserves the durable-execution event kinds — `CheckpointRecorded`, `SignalReceived`, `ApprovalRequested`, `ApprovalGranted`/`ApprovalDenied`, `JobSuspended`, `JobResumed` — plus `JobRepaired` for the repair feature, from day one. Heartbeats and progress percentages are ephemeral and live outside the log, but threshold crossings are events (`JobStalled`).

- **Consequences:**
  - Easier: any job's causal history is queryable and replayable across languages; repair & resume is possible because full prior history is preserved rather than overwritten; phase 5 (durable execution) needs no schema migration since the shape is reserved from v1.
  - Harder: state can never be mutated in place — every correction is a new event (`JobRepaired`, conflict events), which requires discipline throughout the codebase; the job-lifecycle state machine must be proven correct (a proptest ensuring no event sequence reaches an illegal state), which is extra test-authoring burden up front.
  - Gave up: the simplicity of a mutable-state model where updating a row in place is sufficient.

- **Alternatives considered:**
  - Mutable state plus an audit log — rejected: the audit log becomes secondary and advisory rather than authoritative, so it can drift from actual state and cannot be replayed to reconstruct behavior.
  - Hybrid event sourcing for some aggregates only — rejected: undermines the one-event-model claim that lets a classic job be described as the degenerate case (zero checkpoints) of the same model.
