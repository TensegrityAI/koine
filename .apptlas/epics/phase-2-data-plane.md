# Epic: Phase 2 — Data plane

- **State:** planned
- **Implements:** design spec §2 (data plane, contract), §3 (delivery), §6 phase 2
- **Exit criteria:** a real Python worker processes jobs with demonstrable
  crash recovery; the conformance suite passes against the Python SDK; TLC
  verifies the protocol model's stated properties.

## Candidate items

1. **TLA+ model of the lease/delivery protocol** — states: enqueued, leased,
   running, acked, failed, expired; actions: crash, timeout, late ack,
   heartbeat, retry. Properties checked with TLC:
   - *Safety:* no job is ever lost (every enqueued job is eventually acked,
     parked, or cancelled — never vanishes); no two workers hold a valid
     lease on the same job simultaneously; a late ack never silently
     overwrites recorded history (conflict event required).
   - *Liveness (under fairness):* an eligible job is eventually leased.
   Model lives in `docs/formal/` with a README on running TLC. **Written and
   checked BEFORE the gRPC implementation**; divergences found later feed
   back into the model (drift discipline applies to models too).
2. **`koine-proto` v1** — package `koine.v1`; services for worker data plane
   (fetch stream, ack, fail, heartbeat; checkpoint RPC reserved/stubbed);
   ADR: proto versioning & compatibility policy. `build.rs` + tonic codegen.
3. **`koine-grpc` server** — bidirectional/streaming fetch wired to
   `LeaseNextJob`; heartbeats extend leases; deadline/keepalive handling;
   tracing interceptor propagating W3C context from event lineage.
4. **Worker wakeup** — Postgres LISTEN/NOTIFY (or notify-on-append) so fetch
   streams don't poll hot; measured fallback polling interval.
5. **Worker auth v1** — ADR: minimal credible scheme (per-worker token +
   TLS guidance); full OAuth/OIDC is out of scope until a real need.
6. **Python SDK (minimal)** — generated client + thin idiomatic layer:
   worker loop, handler registration, heartbeat thread, graceful shutdown,
   structured failure reporting (`sdks/python/`).
7. **Conformance suite** — language-agnostic harness: spawns broker
   (testcontainers), drives any SDK through fetch/ack/fail/heartbeat/crash
   scenarios derived from the TLA+ properties; Python SDK is the first to
   pass. *(ring 4)*
8. **Crash-recovery demo** — scripted: kill the worker mid-job, watch lease
   expiry → retry → success; this is the phase's end-to-end product exercise.
9. **Benchmarks (baseline)** — enqueue/dispatch throughput + latency on the
   SKIP LOCKED path; recorded in the wiki, not marketed.
10. **crates.io name reservation** — publish 0.1.x with real (if minimal)
    content, per deferred decision; requires backlog item
    `manifest-cleanup-workspace-deps` first.
11. **Wiki pages** — `koine-proto`, `koine-grpc`, `data-plane.md`,
    `formal-models.md`. *(DoD)*

## Dependencies

- Phase 1 complete (use cases + stores are what gRPC adapts).
- TLA+ toolchain (TLC) available locally/CI — decide CI integration scope
  (running TLC in CI vs on-demand) via ADR or plan decision.

## Risks

- Streaming gRPC + lease lifecycle has subtle cancellation paths — the TLA+
  model and conformance suite exist precisely to cage this.
- SDK ergonomics define the polyglot promise's first impression — review the
  Python API with faktory-tools experience before freezing.

## Verification strategy

TLC on the model; ring 3 for adapter internals; ring 4 conformance as the
contract seal; scripted crash demo as product exercise.
