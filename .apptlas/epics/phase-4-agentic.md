# Epic: Phase 4 — Agentic operation

- **State:** planned
- **Implements:** design spec §1 (agent-native thesis), §2 (MCP adapter), §6 phase 4
- **Exit criteria:** an agent (MCP client) enqueues a job, inspects its
  history, and operates the broker (cancel/park/stats) end-to-end via MCP.

## Candidate items

1. **`koine-mcp` server** — rmcp; same use cases the REST adapter drives
   (hexagonal: MCP is a driving adapter, zero business logic). Transports:
   stdio + streamable HTTP.
2. **MCP security** — API-key auth + scopes; **Origin validation
   (DNS-rebinding protection)** on the HTTP transport — the todo-app
   pattern, done properly from day one.
3. **Tools v1** — `enqueue`, `job.get`, `job.history` (the causal event
   chain), `job.cancel`, `job.park`/`unpark`, `queue.stats`,
   `broker.health`. Tool descriptions written for LLM consumers (clear
   contracts, failure modes).
4. **Resources** — live OpenAPI spec, event-taxonomy reference, per-queue
   snapshots: the broker explains itself to agents.
5. **Rich history projections** — correlation-chain search (all jobs sharing
   a `correlation_id`), causation trees; powers both MCP `job.history` and
   the dashboard's causal graph.
6. **Dashboard v1 — the d3 causal trace graph** — the showpiece: a job's
   causal tree rendered as an interactive d3 graph (events, retries,
   lease losses, cross-job causation), dark/data-dense per the design
   language.
7. **Agent E2E demo** — a scripted MCP client (or Claude session) that
   enqueues, watches, and operates; recorded as the phase's product
   exercise.
8. **Wiki pages** — `koine-mcp.md`, `agent-operation.md`; MCP usage guide
   in README. *(DoD)*

## Dependencies

- Phase 3 (use cases, projections, and auth conventions all exist; MCP
  reuses them).

## Risks

- Tool design for agents is UX design — iterate against a real agent early,
  not at the end.
- Security posture must not lag features: HTTP transport ships with auth +
  Origin validation or doesn't ship.

## Verification strategy

Ring 2 for tool→use-case mapping; adapter unit tests incl. auth/Origin
cases; the agent E2E demo is the acceptance instrument.
