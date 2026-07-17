# Policy: Testing

Four rings plus formal methods where they pay. TDD is mandatory across all of
them (see AGENTS.md non-negotiables): failing test first, minimal
implementation, green, commit.

## The four rings

| Ring | Scope | Runs against | Speed |
| --- | --- | --- | --- |
| 1 — Domain | Pure unit tests + **proptest** over state machines (no event sequence may reach an illegal state) | Nothing external — `koine-domain` has no I/O | ms |
| 2 — Application | Use cases end-to-end through the ports | `koine-store-memory` adapters | ms, no Docker |
| 3 — Integration | Store adapters, projections, crash/retry scenarios | testcontainers Postgres running the **real migrations** (`sqlx::migrate!`) — never an inline schema copy | seconds |
| 4 — Conformance | The wire contract every SDK must honor (fetch/ack/fail/heartbeat/checkpoint) | A real broker instance | seconds |

Placement rule: test at the innermost ring that can express the behavior.
A domain invariant tested only through ring 3 is a smell.

## Formal methods — when they pay

- **TLA+**: required when designing or changing distributed-protocol behavior —
  lease acquisition/expiry, delivery semantics, outbox relay, anything where
  the bug lives in an interleaving of crash/timeout/late-ack. Model first,
  implement second (planned for phase 2's data plane).
- **Kani** (Rust model checking): for state-machine and invariant proofs where
  proptest's sampling is not enough — complements, never replaces, ring 1.

## Test hygiene

- Tests are named for the behavior they assert, not the function they call.
- A test that asserts nothing, or asserts the implementation rather than the
  behavior, is a review finding (see
  [../rubrics/code-review-rubric.md](../rubrics/code-review-rubric.md)).
- Every AC in an item declares which ring verifies it
  ([definition-of-ready.md](definition-of-ready.md)).
