# `koine-server`

## What it does

The composition root: the one crate that wires concrete driven adapters to
the application core and owns the production `Clock`/`IdGenerator`
implementations. Today it is a single binary with one subcommand, `dev-loop`,
that drives the whole 1B stack — enqueue, worker, sweep, outbox relay — against
a real Postgres and prints each job's recorded story; that run is the
product-level proof behind the phase's Definition of Done ("exercised as a
product", not only through unit tests).

## How it is built

- **`runtime.rs`** — `SystemClock` (`Clock::now` → `Utc::now()`) and
  `UuidV7Ids` (`IdGenerator` over `Uuid::now_v7()` for every id kind;
  `jitter_seed` folds both halves of a fresh UUIDv7 for high entropy, per the
  port's documented contract). These are the only production implementations
  of the two ports `koine-domain` is never allowed to touch directly (ADR
  0010).
- **`main.rs`** — reads `argv[1]`; `dev-loop` reads `DATABASE_URL` (falling
  back to a local default) and runs `dev_loop::run`, mapping its `Result` to
  `ExitCode::SUCCESS`/`FAILURE`; anything else prints a one-line banner.
- **`dev_loop.rs`** — `connect_pool` then builds one `Arc<PostgresEventStore>`,
  `Arc<PostgresDispatcher<UuidV7Ids, SystemClock>>`, and a
  `PostgresOutboxRelay` over the same pool; `enqueue_dev_jobs` enqueues three
  jobs on queue `"dev"` (plain, `{"crashy": true}`, `{"flaky": true}`);
  `worker_loop` runs as a spawned task, leasing and deciding via the payload
  flag plus `LeasedJob::attempt` — the crashy job's first lease is dropped
  with no ack (simulated crash, recovered only by the sweep), the flaky job
  fails retryably once then succeeds; the main loop ticks a 300ms interval
  driving `sweep.execute()` then `relay.relay_once(&PrintingSink, 64)`, polling
  each job's folded story until all three read `"succeeded"` or a 60s budget
  elapses; `check_stories` asserts job1's story is the exact plain sequence and
  job2/job3 contain their crash/retry markers, collecting every missing marker
  into one descriptive error rather than failing on the first — the binary's
  own acceptance check, baked in rather than left to eyeballing output.

## Why

- ADR 0003 — the hexagon is compiled; `koine-server` is deliberately the only
  crate allowed to see every adapter at once, wiring them behind the ports
  `koine-application` defines.
- The dev-loop's existence is itself the phase's DoD item 2 (exercised as a
  product): a passing `cargo test --workspace` proves the ports compose, but
  only a real run against a real database proves the transactions, the
  `SKIP LOCKED` claim, and the outbox relay behave under an actual crash/retry
  story end to end.

## Boundaries

- Depends on every crate below it in the hexagon: `koine-domain`,
  `koine-application`, and (today) `koine-store-postgres`. `Cargo.toml` also
  declares `koine-store-memory`, `koine-grpc`, `koine-http`, `koine-mcp`, and
  `koine-observability` as dependencies (verifying the phase-0 workspace
  wiring), but nothing in this crate's source references them yet — they are
  inert until the phases that give them real behavior (2–4) wire them in here
  too; neither `cargo deny` nor clippy flag unused *dependencies* (only unused
  imports), so this asymmetry is real but currently invisible to CI.
- Grows with each phase: the gRPC data-plane adapter arrives in phase 2, the
  REST control plane and dashboard in phase 3, the MCP adapter in phase 4 —
  each is wired into this same composition root, not a new one.
- `dev-loop` is a development/exercise command, not a production entry point;
  phase 2+ adds the real server-mode subcommand(s) that actually serve
  traffic.
