# 0009 — Dashboard: Vite+React SPA, embedded

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** The dashboard is deliberately the first "foreign-language" consumer of the control plane, dogfooding the polyglot promise. Its design language target is professional minimalist, a Palantir/Blueprint sensibility — dark, data-dense, precise typography, restrained motion. Delivery must preserve the single-binary deploy: `./koine serve` should give broker and UI together.

- **Decision:** Build the dashboard as a Vite + React + TypeScript SPA in `dashboard/`, with a TS hexagonal structure (ports/adapters) and an API client generated from the OpenAPI spec. Use d3.js for bespoke, data-dense visualizations — queue flows, event timelines, causal trace graphs — rather than a generic chart library, to keep full control over the design bar. Ship it as a static build embedded into `koine-server` via rust-embed, so `./koine serve` remains a single binary; live updates arrive over SSE/WebSocket from the control plane.

- **Consequences:**
  - Easier: the dashboard proves the polyglot/control-plane promise on itself, since its client is OpenAPI-generated rather than hand-rolled; single-binary deploys are preserved end to end; d3 gives full control over the Palantir/Blueprint-grade visual bar the spec sets.
  - Harder: a Node/npm build pipeline must run at release time even though the shipped artifact is a single Rust binary; bespoke d3 visualizations are more implementation work than a generic chart library and must be maintained by hand as views grow.
  - Gave up: Leptos's pure-Rust stack, and the existing todo-app Next.js frontend.

- **Alternatives considered:**
  - Leptos — rejected: painful d3 interop, slow visual iteration, and an immature component ecosystem for the design bar the dashboard targets.
  - Copying the todo-app's Next.js frontend — rejected: brings 26k LOC of todo-specific logic, an Apollo/GraphQL layer Koiné doesn't expose, and a Node runtime that would break single-binary deploys.
