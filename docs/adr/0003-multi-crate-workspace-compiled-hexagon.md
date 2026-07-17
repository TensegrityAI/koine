# 0003 — Multi-crate workspace, compiled hexagon

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** A 2026-07-16 audit of kineticrs, the project's own reference codebase, found that a single-crate layout lets hexagonal layering erode over time: kineticrs's `EventStore` trait ended up concretely bound to its `TodoEvent` aggregate, because nothing but convention prevented it. Koiné's solo-maintainer bus-factor risk makes structural, not just conventional, enforcement of boundaries important.

- **Decision:** Structure Koiné as a Cargo workspace where hexagonal boundaries are crate boundaries: `koine-domain` (domain, no async/I/O), `koine-application` (use cases and ports), `koine-proto` (versioned contract), `koine-store-postgres`/`koine-store-memory` (driven adapters), `koine-grpc`/`koine-http`/`koine-mcp` (driving adapters), `koine-observability` (infra), and `koine-server`/`koine-cli` (binaries). The dependency graph makes architecture violations impossible to compile — `koine-domain` cannot depend on sqlx.

- **Consequences:**
  - Easier: architecture violations are caught by `cargo build`, not by code-review discipline alone; new inter-crate dependency edges are visible and deliberate, and require an ADR to introduce.
  - Harder: more crates to create, version, and document than a single crate; more `Cargo.toml` boilerplate and cross-crate visibility to manage compared to internal module discipline.
  - Gave up: the simplicity of a single compilation unit.

- **Alternatives considered:**
  - Single crate with module-level discipline — rejected: this is the kineticrs approach (kineticrs ADR-0003), and its audited failure mode is exactly that layering erodes because nothing but convention enforces it.
