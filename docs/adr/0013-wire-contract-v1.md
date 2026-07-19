# 0013 — Wire contract v1

- **Status:** accepted
- **Date:** 2026-07-19
- **Context:** The data plane needs its versioned contract (spec §2: "the
  proto IS the product's polyglot promise"). Decisions needed: RPC shape,
  payload encoding, evolution policy, and the new compile-time edge
  `koine-grpc → koine-proto`.
- **Decision:**
  - Package **`koine.v1`**, one file `koine/v1/worker.proto` (data plane
    only; the control plane is REST in phase 3).
  - **RPC shape: server-streaming `Fetch` + unary control ops**
    (`Start`, `Succeed`, `Fail`, `Heartbeat`). The server streams
    `LeasedJob`s as they become claimable (long-lived stream, wakeup via
    `DispatchSignal`); acks are unary because they are individually
    meaningful, retryable, and map 1:1 to use cases. The spec §2 diagram
    says "bidi-stream"; a full bidi protocol multiplexing acks into the
    stream adds session-state complexity v1 does not need — recorded as a
    documented divergence; re-evaluate with phase-2B benchmarks.
  - **Payloads are JSON strings** (`payload_json`, `result_json`,
    structured `JobError` fields): mirrors the JSONB source of truth
    (ADR 0010), keeps every SDK trivial (parse JSON, no nested proto
    schema per job type). Timestamps are `int64` unix milliseconds
    (`expires_at_unix_ms`) — no well-known-type dependency in SDKs.
  - **Evolution: additive-only within v1.** New fields = new numbers,
    optional semantics; removed fields become `reserved N;` forever; field
    numbers and names are wire contract like event kinds. Breaking changes
    = `koine.v2` package side-by-side. The ring-4 conformance suite (2B)
    is the compatibility gate.
  - New dependency edge `koine-grpc → koine-proto` (and `koine-server →`
    both) — recorded here per AGENTS.md's edge rule.
- **Consequences:** SDKs are generated + thin; JSON payload cost accepted
  until benchmarks argue otherwise; the stream's lease TTL is chosen by the
  worker per Fetch (bounded server-side); divergence from the spec diagram
  is on record with its revisit trigger.
- **Alternatives considered:** full bidi stream (session-state complexity,
  ack ordering ambiguity); unary long-poll `LeaseNext` (simplest, but
  per-claim RTT and no server push); protobuf-native payloads (couples every
  job type to proto schema churn); google.protobuf.Timestamp/Struct (drags
  well-known types into every SDK).
