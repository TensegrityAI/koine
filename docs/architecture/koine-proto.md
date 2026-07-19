# `koine-proto`

## What it does

The `koine.v1` data-plane wire contract (ADR 0013): one protobuf file
defining `WorkerService` — the RPCs a worker in any language calls to fetch
jobs and report their outcome — plus the generated Rust client/server types.
It is the polyglot promise made concrete: any language with a gRPC codegen
tool can point it at `proto/koine/v1/worker.proto` and get a working Koiné
worker client.

## How it is built

- **`proto/koine/v1/worker.proto`** — package `koine.v1`, one service:
  `rpc Fetch(FetchRequest) returns (stream LeasedJob)` (server-streaming;
  long-lived, wakeup via `DispatchSignal`) plus four unary control ops —
  `Start`, `Succeed`, `Fail`, `Heartbeat`. Messages: `FetchRequest`
  (`queue`, `lease_ttl_ms`), `LeasedJob` (`job_id`, `queue`,
  `payload_json`, `attempt`, `lease_id`, `expires_at_unix_ms`,
  `correlation_id`, optional `traceparent`), `StartRequest`/`Response`,
  `SucceedRequest` (`job_id`, `lease_id`, optional `result_json`),
  `FailRequest` (`job_id`, `lease_id`, `JobError`), `JobError` (`kind`,
  `message`, optional `stacktrace`, `retryable`), the `AckOutcome` enum
  (`UNSPECIFIED`/`RECORDED`/`CONFLICT`) carried in `AckResponse`, and
  `HeartbeatRequest`/`Response` (`ttl_ms` in, `alive` bool out).
- **Payloads are JSON strings, not nested proto messages**
  (`payload_json`, `result_json`) — mirrors the JSONB source of truth
  (ADR 0010) so every SDK does "parse JSON", never schema-per-job-type
  codegen. Timestamps are `int64` unix milliseconds
  (`expires_at_unix_ms`), avoiding a `google.protobuf.Timestamp`
  dependency in every SDK.
- **Codegen** (`build.rs`) — `tonic_prost_build::configure().build_server
  (true).build_client(true).compile_protos(...)` against tonic 0.14.6 /
  prost 0.14.4 (`tonic-prost` 0.14.6), generating into `OUT_DIR`.
  `src/lib.rs` re-exports the generated module as `pub mod v1` via
  `tonic::include_proto!("koine.v1")`, annotated
  `#[allow(missing_docs, clippy::pedantic, clippy::nursery)]` since it's
  generated code the crate doesn't hand-author.
- **Evolution: additive-only within v1** (ADR 0013) — new fields get new
  numbers with optional semantics; a removed field becomes `reserved N;`
  forever; field numbers and names are permanent wire contract, exactly
  like event kinds. A breaking change is a new `koine.v2` package
  side-by-side, never an in-place renumbering of v1. The ring-4
  conformance suite (2B) is the compatibility gate that exercises this in
  practice.
- **`cargo-machete` false positive, documented** — `prost` and
  `tonic-prost` are referenced only through the `include!()` that
  `tonic::include_proto!` performs at compile time from `OUT_DIR`, which
  machete's static source scan never sees. `Cargo.toml` carries
  `[package.metadata.cargo-machete] ignored = ["prost", "tonic-prost"]`
  with a comment recording why (`phase-2-carryover-hardening` AC3), so the
  workspace-wide machete gate stays green without masking a real unused
  dependency.

## Why

- ADR 0013 — RPC shape (server-streaming `Fetch` + unary acks), JSON
  payload encoding, additive-only evolution policy, and the new
  compile-time edge `koine-grpc → koine-proto` (and `koine-server →`
  both).
- ADR 0014 — auth rides request metadata (`authorization`,
  `koine-worker-id`), not proto fields; the contract itself carries no
  credentials.

## Boundaries

- **Zero dependency on any other Koiné crate** — the contract is
  standalone by design so any language's codegen can consume
  `proto/koine/v1/worker.proto` directly, with no Rust in the loop.
- Depended on by `koine-grpc` (implements `WorkerService` against it) and
  `koine-server` (wires the generated server type into `serve`).
- **The design spec's §2 diagram shows a bidi-stream**; this contract
  ships server-streaming `Fetch` + unary acks instead — a documented
  divergence (ADR 0013), not a silent one, with its revisit tied to
  phase-2B benchmarks.
- No `Enqueue` RPC exists on this contract — jobs enter the event log
  out-of-band (through the use cases directly); the data plane only ever
  hands out and acks already-enqueued jobs. (The crate-level doc comment
  in `Cargo.toml`/`src/lib.rs` also mentions "checkpoints"; no checkpoint
  RPC exists in `worker.proto` or anywhere in this branch — that phrase is
  aspirational wording left over in the doc comment, not delivered
  behavior, and should not be read as a real capability.)
