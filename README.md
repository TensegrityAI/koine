# Koiné

> The common language between programming languages for background work.

**Koiné** (κοινή, *"the common [language]"*) is an event-sourced, language-agnostic
job broker written in Rust. The history of every job is the source of truth, not a
byproduct — which yields three capabilities no open-source broker combines:

- **Total traceability** — what happened, why, in what order, with what context;
  queryable and replayable, across languages.
- **Repair & resume** — a failed job is not "retried and hope": it is inspected,
  repaired, and continued from its last checkpoint, full history preserved.
- **Agent-native operation** — the control plane speaks MCP; agents are first-class
  operators, not an afterthought.

Workers connect over a versioned gRPC contract — SDKs are generated, not
reverse-engineered. Producers and operators use REST (OpenAPI), the CLI, or MCP.

## Status

**Pre-alpha.** Phase 0 (foundations) in progress. See
[`docs/superpowers/specs/2026-07-16-koine-design.md`](docs/superpowers/specs/2026-07-16-koine-design.md)
for the full design and build phases.

## Building

```bash
cargo build --workspace
cargo test --workspace
```

Requires the toolchain pinned in `rust-toolchain.toml` (rustup handles this
automatically).

## Architecture at a glance

Strict hexagonal architecture as an 11-crate workspace — the dependency graph *is*
the architecture guardian. Event log on Postgres as single source of truth;
synchronous dispatch projection for the hot path; transactional outbox for read
projections. See the design spec and `docs/adr/` for every decision and its
rationale.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
