# `koine-grpc`

## What it does

The data-plane driving adapter: `WorkerApi` implements the generated
`WorkerService` trait (`koine-proto`), bridging the `koine.v1` wire contract
to phase 1's use cases (`LeaseNextJob`, `WorkerAck`, `Heartbeat`). It is the
one crate a worker in any language actually talks to.

## How it is built

- **`src/service.rs`** — `Deps<S, D, G, C, Sig, P>` bundles the six ports a
  worker surface needs (`EventStore`, `Dispatcher`, `IdGenerator`, `Clock`,
  `DispatchSignal`, `WorkerPresence`) plus `GrpcConfig` (bearer `token`,
  `max_lease_ttl` ceiling every requested lease is clamped to, `idle_poll`
  fallback interval). `WorkerApi<S, D, G, C, Sig, P>` wraps
  `Arc<Deps<...>>`; `server(deps)` builds the tonic
  `WorkerServiceServer<WorkerApi<...>>` ready for `.add_service()`.
- **Auth is a plain function called at the top of every handler, not a
  tonic `Interceptor` layer** — `auth::check(request.metadata(),
  &config.token)` is the first line of `fetch`/`start`/`succeed`/`fail`/
  `heartbeat`; there is no middleware wrapping the service. It validates
  `authorization: Bearer <token>` with a constant-time comparison
  (`subtle::ConstantTimeEq`, length check then `ct_eq`) and
  `koine-worker-id` (parsed through the domain's `WorkerId::new`:
  non-empty, ≤256 bytes, no control characters). **An empty configured
  token unconditionally rejects every caller**, including one presenting an
  equally empty `"Bearer "` (trailing-space, no credential) token — this
  closes the case where a
  length-then-`ct_eq` check alone would treat `0 == 0` as a match (a
  security fix from review). Every failure returns `UNAUTHENTICATED` with a
  fixed `"invalid credentials"` message — no detail about which check
  failed leaks to the caller.
- **`fetch` spawns one detached task per stream** — it loops
  `LeaseNextJob::execute`; on `Some(job)` it sends the wire-converted job
  over an `mpsc::channel(16)` feeding the `ReceiverStream` response; on
  `None` it races `tokio::select!` between `deps.signal.wait(&queue,
  idle_poll)` and `tx.closed()`. Racing `tx.closed()` is a leak fix from
  review: without it, a client that drops the stream while its queue is
  idle leaves the spawned task polling forever, because it's parked in
  `signal.wait`, never observing the closed receiver — regression-tested by
  `fetch_task_ends_when_receiver_drops_while_idle`. A worker that
  disconnects between a successful claim and the send loses nothing: the
  job is already leased and durably appended (ADR 0011), so the lease
  simply expires and the sweep reclaims it — no special-cased recovery
  path needed.
- **Acks are thin** — `start`/`succeed`/`fail`/`heartbeat` parse wire
  UUIDs, call the matching use case, map its `Result` to the wire shape.
  `map_ack_error` turns a domain rejection into `failed_precondition`
  ("job state no longer permits this operation; refetch"),
  `StreamNotFound` into `not_found`, any other store error into an opaque
  `internal`. `to_proto_outcome` maps `AckOutcome::Recorded`/`Conflict` to
  the wire `AckOutcome` enum.
- **Presence is recorded synchronously**, before either spawns/executes its
  use case: `fetch` calls `presence.seen(&worker, Some(&queue))`, `start`
  calls `presence.seen(&worker, None)`. So even a `fetch` against an empty
  queue registers the worker — proven by `presence_rows_appear` (no job
  needs to exist for the assertion to hold).
- **TLS is proxy-terminated, not native (ADR 0014) — stated honestly**:
  this crate binds plain HTTP/2 via `tonic::transport::Server`; there is no
  rustls/mTLS wiring anywhere in this crate or in `koine-server`.
  Deployment guidance is to put a TLS-terminating ingress in front — the
  crate makes no other claim.

## Why

- ADR 0013 — the RPC shape (server-streaming `Fetch` + unary acks) and its
  documented divergence from the design spec's bidi-stream diagram; JSON
  payloads; additive-only wire evolution.
- ADR 0014 — the whole auth model: single shared bearer token per
  deployment, worker identity claimed-not-proven, constant-time comparison,
  proxy-terminated TLS.
- ADR 0015 — presence is read/written only through the `WorkerPresence`
  port, an ephemeral-state port with no domain events; this crate is one of
  its two callers (`koine-server`'s `serve` is the other, via the same
  `Deps` shape).

## Boundaries

- Depends on `koine-domain`, `koine-application` (the ports and use cases
  it wires) and `koine-proto` (the generated service trait/types it
  implements).
- Depended on by `koine-server` (`serve.rs` wires `PgSignal`/`PgPresence`/
  `PostgresEventStore`/`PostgresDispatcher` into `Deps` and calls
  `koine_grpc::server(deps)`).
- **Test suites, by tier** — `tests/wire.rs` (6 tests, real tonic transport
  over an in-process `tokio::io::duplex` pair, in-memory adapters):
  `unauthenticated_calls_are_rejected`, `fetch_streams_a_claimed_job`,
  `full_story_over_the_wire`, `stale_ack_returns_conflict`,
  `fetch_wakes_on_late_enqueue` (proves signal-driven wakeup against a 10s
  idle-poll ceiling so a sub-second wake can only have come from the
  signal), `heartbeat_reports_liveness`. `tests/fetch_idle_disconnect.rs`
  (1 test, `fetch_task_ends_when_receiver_drops_while_idle`, drives
  `WorkerApi::fetch` directly as a trait method with a call-counting
  `Dispatcher` wrapper to prove the leak fix). `tests/grpc_e2e.rs` (2
  tests, real TCP socket + real Postgres): `crash_recovery_over_the_wire`,
  `presence_rows_appear` — its `tests/support/mod.rs` `SystemClock`/
  `UuidV7Ids` are a verbatim copy of `koine-server/src/runtime.rs`'s types
  (`koine-server` is bin-only and so cannot be a dev-dependency; the
  duplication is a recorded phase-2B dedup follow-up, not an oversight).
- No `Enqueue` RPC — a worker only ever fetches/acks jobs someone else
  enqueued through the use cases directly.
- No checkpoint RPC exists in `koine-proto`'s `worker.proto` or anywhere in
  this crate.
