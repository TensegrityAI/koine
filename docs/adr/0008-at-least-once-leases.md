# 0008 — At-least-once delivery with leases

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** A worker should never simply "receive" a job with no way to detect that it died mid-work; delivery semantics need to guarantee no silent loss while staying honest about what distributed systems can actually promise.

- **Decision:** Delivery is at-least-once via leases: a worker acquires a lease with a TTL, renewed by heartbeat over the gRPC stream, instead of receiving a job outright. If a worker dies, the lease expires, the job becomes eligible again, and `LeaseExpired` records exactly what happened. A late ACK after expiry is recorded as an explicit conflict event rather than discarded. Retries use exponential backoff plus jitter per the retry policy declared at enqueue; exhausted retries move the job to `parked`, with full history, awaiting repair. Heartbeats and progress percentages are ephemeral outside the log, but threshold crossings are events (`JobStalled`).

- **Consequences:**
  - Easier: no job is ever silently lost on worker crash, and every ambiguous outcome — a late ack, a stall — becomes a queryable event rather than a mystery.
  - Harder: at-least-once means a job can be delivered more than once, so worker and checkpoint logic must tolerate duplicate execution — idempotency is the worker's responsibility, not a broker guarantee — and the broker must track lease TTLs and heartbeats for every in-flight job.
  - Gave up: the false simplicity of promising exactly-once delivery.

- **Alternatives considered:**
  - At-most-once — rejected: a crashed worker means the job is simply lost, contradicting total traceability and "no silent loss."
  - Exactly-once — rejected as a dishonest claim: distributed exactly-once delivery is not achievable in practice, and asserting it would mislead operators.
