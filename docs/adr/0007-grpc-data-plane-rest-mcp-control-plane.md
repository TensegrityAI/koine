# 0007 — gRPC data plane; REST+MCP control plane

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** Koiné separates two planes: a control plane for producers, operators, and agents (casual clients, low volume) and a data plane for workers (high volume, long-lived, bidirectional). Faktory's lack of a first-class protocol contract forces clients to reverse-engineer its wire protocol — exactly the failure mode that motivated faktory-tools, and the one Koiné must avoid.

- **Decision:** gRPC is the canonical data-plane protocol: `.proto` files are a versioned, first-class contract, and official codegen gives every language a typed client without reverse-engineering a wire protocol. REST with OpenAPI, an MCP adapter, and a CLI serve the control plane.

- **Consequences:**
  - Easier: every worker language gets a typed client "for free" from the shared `.proto`, directly closing the failure mode that motivated faktory-tools; a protocol conformance suite can validate any SDK against one contract.
  - Harder: two protocol stacks must be built and maintained instead of one — gRPC/tonic for the data plane, REST/OpenAPI plus MCP plus CLI for the control plane — and the `.proto` contract must be versioned carefully since it is the source of every worker SDK.
  - Gave up: the simplicity of a single protocol serving both planes.

- **Alternatives considered:**
  - WebSocket + JSON as the canonical protocol — rejected: no official per-language codegen or typed contract, reintroducing the reverse-engineering problem gRPC avoids.
  - Both protocols first-class from v1 — rejected: doubles the data-plane surface to build and conformance-test before either is solid.
