# Epic: Phase 3 — Control plane + dashboard v0

- **State:** planned
- **Implements:** design spec §2 (control plane), §5 (dashboard), §6 phase 3
- **Exit criteria:** OTel traces visible producer→worker cross-language; the
  dashboard shows live queues/history over SSE, served from the single
  `koine-server` binary.

## Candidate items

1. **REST API (`koine-http`)** — axum + utoipa/OpenAPI: enqueue, job status,
   per-job event history, queue stats, cancel, park/unpark; pagination and
   stable error envelope; OpenAPI served at a well-known path.
2. **Live updates** — SSE (default; WS only if SSE proves insufficient)
   streaming queue stats and job events off the read projections.
3. **`koine-cli`** — `koine trace <job_id>` (the causal history, cross-
   language), `koine enqueue`, `koine queues`, `koine job <id>`; talks REST;
   output honest and scriptable (`--json`).
4. **Observability (`koine-observability`)** — OTel OTLP tracing wired
   through server + SDK trace propagation (the traceparent carried in events
   since phase 1 pays off here); Prometheus `/metrics`; dashboard-relevant
   metrics (queue depth, lease expiries, retry/park rates, outbox lag).
5. **History projections** — read models the API/dashboard need: per-job
   event timeline, per-queue stats, recent activity. All rebuildable (ring 3
   replay test extends to them).
6. **Dashboard scaffold** — `dashboard/`: Vite + React + TS, hexagonal TS
   structure (ports/adapters), client **generated from the OpenAPI spec**
   (the first "foreign language" consumer), CI jobs: lint/test/build.
7. **Dashboard v0 views** — live queue board, job detail with event
   timeline, park list. Design language per ADR 0009: dark, data-dense,
   Palantir/Blueprint sensibility; d3 for the first bespoke viz (queue flow
   sparkline-level, the causal graph is phase 4).
8. **Embedding** — rust-embed of the static build into `koine-server`;
   `./koine serve` = broker + UI; build pipeline decision (dashboard build
   artifact committed vs built in CI) via ADR.
9. **auth v1 for control plane** — ADR: minimal credible (API key headers),
   consistent with the phase-2 worker auth decision.
10. **mdBook decision** — migrate `docs/architecture/` to a docs site now
    that pages exist in quantity, or defer; ADR either way.
11. **Wiki pages** — `koine-http`, `koine-cli`, `koine-observability`,
    `dashboard.md`, `control-plane.md`. *(DoD)*

## Dependencies

- Phase 2 (traces flow producer→worker only once the data plane exists;
  the dashboard's liveliest data is worker activity).
- Node toolchain in CI for the dashboard jobs (already available locally).

## Risks

- Dashboard scope creep is the classic sink — v0 is three views, deliberately;
  the d3 showpiece (causal graph) is *phase 4* where it has real data.
- OpenAPI-generated TS client quality varies by generator — pick early, in
  the plan's first dashboard task.

## Verification strategy

Ring 2/3 for API + projections; dashboard unit tests (vitest) + one
playwright-level smoke against the embedded build; the phase's product
exercise is `./koine serve` + a worker + the dashboard showing it live.
