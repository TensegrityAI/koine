# Epic: Phase 2 — Data plane

- **State:** in progress — phase 2A done (items 1–6 delivered); phase 2B next
  (items 7–12: Python SDK, conformance suite, scripted crash demo, benchmarks,
  crates.io publication, `tests/support` dedup)
- **Implements:** design spec §2 (data plane, contract), §3 (delivery), §6 phase 2
- **Exit criteria:** a real Python worker processes jobs with demonstrable
  crash recovery; the conformance suite passes against the Python SDK; TLC
  verifies the protocol model's stated properties. TLC verification and a
  real gRPC server with crash recovery proven over a socket+Postgres are done
  (2A); the Python worker and conformance suite are not (2B) — the epic's
  exit criteria are not yet fully met.

## Candidate items

1. **DONE (2A)** — **TLA+ model of the lease/delivery protocol** —
   `docs/formal/lease_protocol.tla`/`.cfg`, TLC-checked in CI (the `tla`
   job) and via `make tla`. Delivered with 7 invariants (`TypeOK`,
   `NoDualLease`, `FreshLeases`, `AttemptCapped`, `LeaseFencingOK`,
   `NoLeaseWhenIdle`, `NonRetryableAlwaysParks`) plus the `EventuallySettled`
   liveness property — `LeaseFencingOK`/`NoLeaseWhenIdle`/
   `NonRetryableAlwaysParks` strengthen this item's original safety-property
   sketch (ghost-variable fencing, quiescence, and the retryable/
   non-retryable split were added during the fix round, not scoped in this
   text originally). See `docs/formal/README.md`.
2. **DONE (2A)** — **`koine-proto` v1** — package `koine.v1`, one file
   `worker.proto`; `Fetch` (server-streaming) + unary `Start`/`Succeed`/
   `Fail`/`Heartbeat`; ADR 0013 covers versioning & compatibility (additive-
   only, `koine.v2` for breaks). `build.rs` + `tonic-prost-build` (tonic
   0.14.6/prost 0.14.4) codegen. No checkpoint RPC — this item's "reserved/
   stubbed" checkpoint mention was not carried into the delivered contract.
3. **DONE (2A)** — **`koine-grpc` server** — `WorkerApi` wires `Fetch` to
   `LeaseNextJob` (server-streaming, not bidirectional — ADR 0013 documents
   the divergence from this item's original "bidirectional/streaming"
   framing); `Heartbeat` extends leases via the `Dispatcher` port. No W3C
   tracing interceptor exists — `traceparent` rides the wire message
   (`LeasedJob.traceparent`) carried from event lineage, not middleware; a
   real tracing interceptor is not built.
4. **DONE (2A)** — **Worker wakeup** — `PgSignal` (Postgres `LISTEN`/
   `NOTIFY`, in-transaction on the append that lands a job back in
   `Pending`) plus an `idle_poll` fallback (default 1s, `koine-server
   serve`'s `KOINE_IDLE_POLL_MS`); proven by `fetch_wakes_on_late_enqueue`.
   The fallback interval is a chosen default, not a benchmarked one —
   benchmarking is item 10, deferred to 2B.
5. **DONE (2A)** — **Worker auth v1** — ADR 0014: single shared bearer
   token (`KOINE_WORKER_TOKEN`) + claimed worker identity, constant-time
   comparison, proxy-terminated TLS (not native — documented honestly, not
   claimed as more).
6. **DONE (2A)** — **`WorkerRegistration`** — ADR 0015 resolves this as
   ephemeral infrastructure state (`event_store.workers`, upserted per
   authenticated call), not an event-sourced aggregate — a disposition, not
   a literal implementation of this item's original aggregate framing.
7. **→ 2B** — **Python SDK (minimal)** — not started.
8. **→ 2B** — **Conformance suite** *(ring 4)* — not started.
9. **Partially touched, → 2B for the real product exercise** —
   **Crash-recovery demo** — `koine-grpc`'s `crash_recovery_over_the_wire`
   (ring-3 e2e, real TCP socket + real Postgres) proves the exact
   crash → sweep → retry → success arc as a test, but this item's "scripted
   demo… against the SDK" needs the SDK (item 7), which doesn't exist yet.
10. **→ 2B** — **Benchmarks (baseline)** — not started.
11. **→ 2B (or later)** — **crates.io name reservation** — not started;
    still blocked on `manifest-cleanup-workspace-deps`.
12. **DONE (2A)** — **Wiki pages** — `koine-proto.md`, `koine-grpc.md`
    added; `koine-application`/`koine-store-memory`/`koine-store-postgres`/
    `koine-server`/`overview`/`README` updated for the ports, adapters, and
    `serve` command 2A actually shipped. `data-plane.md`/`formal-models.md`
    as separate pages were not created — `docs/formal/README.md` already
    covers the formal-model content, and the per-crate pages plus
    `overview.md`'s data-plane section cover the data-plane content, so a
    dedicated `data-plane.md` was judged redundant rather than omitted by
    oversight.

## Dependencies

- Phase 1 complete (use cases + stores are what gRPC adapts).
- TLA+ toolchain (TLC) available locally/CI — decide CI integration scope
  (running TLC in CI vs on-demand) via ADR or plan decision.

## Risks

- Streaming gRPC + lease lifecycle has subtle cancellation paths — the TLA+
  model and conformance suite exist precisely to cage this.
- The model's subject matter (lease/expiry/late-ack) is partially implemented
  in phase 1 — a TLC counterexample here back-propagates as a phase-1
  fidelity finding, exactly like phase 5's schema clause. Mitigation: draft
  the model skeleton during phase 1's state-machine work so the two co-evolve.
- SDK ergonomics define the polyglot promise's first impression — review the
  Python API with faktory-tools experience before freezing.

## Verification strategy

TLC on the model; ring 3 for adapter internals; ring 4 conformance as the
contract seal; scripted crash demo as product exercise.
