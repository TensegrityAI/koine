# 0005 — Postgres event store behind a port

- **Status:** accepted
- **Date:** 2026-07-16

- **Context:** `koine-application` defines the ports (traits) the domain depends on, including `EventStore`; `koine-store-postgres` and `koine-store-memory` are driven adapters behind that port. kineticrs's Postgres event store and snapshot logic are reused as a reference implementation, rewritten generically since kineticrs's `EventStore` trait was concretely bound to its aggregate. Postgres hot-path throughput is a known risk to Koiné; keeping the store behind a port is what allows a future embedded backend if that risk materializes.

- **Decision:** `EventStore` is a port. The first adapter is Postgres — transactional, supports LISTEN/NOTIFY, battle-tested. A complete in-memory adapter (`koine-store-memory`) is built alongside it for tests, guaranteeing the port stays neutral rather than secretly Postgres-shaped.

- **Consequences:**
  - Easier: application/use-case tests run against `koine-store-memory` with no Docker, fast; if Postgres throughput becomes a ceiling later, an alternate backend can be built behind the same port without touching `koine-domain` or `koine-application`.
  - Harder: every real deployment requires operating a Postgres instance — the in-memory adapter is test-only, not a production alternative — so self-hosting Koiné carries a hard external dependency from day one.
  - Gave up: a from-scratch embedded log, and the flexibility of picking a backend per deployment from v1.

- **Alternatives considered:**
  - A custom embedded log built first — rejected: this is a project in itself (a database engine) and would delay everything behind it.
  - Both Postgres and an embedded backend from day one — rejected: doubles the surface area to build and test before a single adapter is solid; the port already leaves room to add a second adapter later.
