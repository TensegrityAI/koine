# Koiné

> The common language between programming languages for background work.

**Koiné** (κοινή, *"the common [language]"*) is a pre-alpha,
event-sourced job broker written in Rust. The append-only history of each job
is its source of truth, and workers use a versioned gRPC contract rather than
an implementation-specific queue protocol.

## Status

**Pre-alpha.** phase 2A implementation complete; zero-debt hardening active; phase 2B blocked. The remaining phase-2A exit gate is tracked by
[`phase-2a-operational-closure.md`](.apptlas/backlog/ongoing/phase-2a-operational-closure.md).

Available today:

- the event-sourced domain and application core, with complete in-memory and
  Postgres adapters;
- an append-only event log, synchronous dispatch projection, and transactional
  outbox;
- the authenticated `koine.v1` gRPC worker surface: server-streaming `Fetch`
  plus unary `Start`, `Succeed`, `Fail`, and `Heartbeat`;
- at-least-once leases, heartbeat renewal, atomic expiry fencing, crash
  recovery, retries, parking, and explicit late-ack conflicts;
- a TLC-checked TLA+ model of lease identity, heartbeat, expiry, retry, and
  conditional recovery liveness.

Planned, not available today:

- Python SDK, ring-4 conformance, SDK crash demo, benchmarks, and a publication
  decision — phase 2B;
- REST/OpenAPI, the operator CLI, observability, and the embedded dashboard —
  phase 3;
- the MCP control plane — phase 4;
- checkpoints, signals, approvals, and repair/resume — phase 5.

See [ROADMAP.md](ROADMAP.md) for sequencing and the
[approved design](docs/superpowers/specs/2026-07-16-koine-design.md) for the
full product direction.

## Building

The repository pins Rust 1.95.0 in `rust-toolchain.toml`; rustup selects it
automatically. Protobuf compilation uses the exact vendored compiler selected
by `koine-proto`, so no system `protoc` installation is required.

```bash
cargo build --workspace
cargo test --workspace
```

## Running

The phase-2A server requires Postgres and a non-empty worker bearer token. The
repository-owned Compose service matches the default `DATABASE_URL`:

```bash
docker compose up -d postgres
KOINE_WORKER_TOKEN=local-development-token \
  cargo run -p koine-server -- serve
```

The server listens on `0.0.0.0:7419` by default. Copy the variables from
[`.env.example`](.env.example) to configure its database, address, lease
ceiling, idle-poll fallback, and bounded Postgres pool. There is no enqueue or
control-plane RPC yet; `cargo run -p koine-server -- dev-loop` is the current
product exercise that enqueues jobs directly through the application use case
and drives the Postgres lifecycle end to end.

## Architecture at a glance

Strict hexagonal architecture is enforced as an 11-crate workspace: the
dependency graph is the architecture guardian. Seven crates have implemented
phase-2A behavior; the HTTP, CLI, MCP, and observability crates remain explicit
future-phase stubs. Start with the
[architecture wiki](docs/architecture/README.md) and
[ADR index](docs/adr/INDEX.md).

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
