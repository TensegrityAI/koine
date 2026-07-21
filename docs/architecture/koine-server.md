# `koine-server`

## What it does

The composition root: the one crate that wires concrete driven adapters to
the application core and owns the production `Clock`/`IdGenerator`
implementations. It is a single binary with two subcommands: `dev-loop`,
which drives the 1B stack — enqueue, worker, sweep, outbox relay — against a
real Postgres and prints each job's recorded story (the product-level proof
behind phase 1B's Definition of Done), and (phase 2A) `serve`, the real
authenticated `gRPC` data-plane server: it wires the Postgres adapters to
`koine-grpc`'s `WorkerService`, keeps leases and dispatch honest with two
background tickers, and serves worker traffic until `Ctrl-C`.

## How it is built

- **`runtime.rs`** — `SystemClock` (`Clock::now` → `Utc::now()`) and
  `UuidV7Ids` (`IdGenerator` over `Uuid::now_v7()` for every id kind;
  `jitter_seed` folds both halves of a fresh UUIDv7 for high entropy, per the
  port's documented contract). These are the only production implementations
  of the two ports `koine-domain` is never allowed to touch directly (ADR
  0010).
- **`main.rs`** — reads `argv[1]`; `dev-loop` reads `DATABASE_URL` (falling
  back to a local default) and runs `dev_loop::run`; `serve` (phase 2A) runs
  `serve::run` with no arguments (its own configuration comes entirely from
  environment variables); either maps its `Result` to
  `ExitCode::SUCCESS`/`FAILURE`; anything else prints a one-line banner
  listing both commands.
- **`serve.rs`** (phase 2A) — `parse_config` reads `KOINE_WORKER_TOKEN`
  (required — a missing *or empty* value refuses to start, since shell
  interpolation of an unset variable silently yields `""` and starting
  anyway would launch a server whose auth is quietly disabled), plus
  optional `DATABASE_URL`, `KOINE_GRPC_ADDR` (default `0.0.0.0:7419`),
  `KOINE_MAX_LEASE_TTL_MS` (default 300000 — the ceiling every requested
  lease is clamped to), `KOINE_IDLE_POLL_MS` (default 1000 — the drained
  `Fetch` correctness fallback), `KOINE_DB_MAX_CONNECTIONS` (default 16),
  and `KOINE_DB_ACQUIRE_TIMEOUT_MS` (default 5000). Every duration and the
  pool size must be non-zero; malformed or zero values refuse startup before
  any connection, listener, ticker, or socket is created. `run` then:
  connects/migrates via `connect_pool`, establishes `PgSignal`'s one
  dedicated listener with the configured acquisition timeout, and spawns two
  detached `tokio::spawn` tickers on a shared
  500ms `tokio::time::interval` — one running `SweepExpiredLeases` (reclaims
  expired leases so a crashed worker's job becomes claimable again with no
  separate process, ADR 0008), one running `PostgresOutboxRelay::relay_once`
  into `PrintingSink` at a 64-envelope batch size; builds `Deps` from
  `PostgresEventStore`/`PostgresDispatcher`/`PgSignal`/`PgPresence`/
  `UuidV7Ids`/`SystemClock` plus the parsed `GrpcConfig`; and serves
  `koine_grpc::server(deps)` via `tonic::transport::Server::builder()
  .serve_with_shutdown(addr, ctrl_c)` — a clean shutdown on `Ctrl-C`, not a
  hard kill.
- **`sinks.rs`** (phase 2A) — `PrintingSink`, the `EventSink` that prints
  each delivered envelope's stream/version/kind, promoted out of `dev_loop.rs`
  into its own module so both `dev-loop` and `serve` share the one
  implementation instead of two copies.
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
- The dev-loop's existence is itself phase 1B's DoD item 2 (exercised as a
  product): a passing `cargo test --workspace` proves the ports compose, but
  only a real run against a real database proves the transactions, the
  `SKIP LOCKED` claim, and the outbox relay behave under an actual crash/retry
  story end to end.
- ADR 0014 — `serve` refuses to start without `KOINE_WORKER_TOKEN`: the data
  plane must never run unauthenticated, so a misconfigured deployment fails
  fast at startup instead of quietly serving traffic with a broken auth
  check.

## Boundaries

- Depends on every crate below it in the hexagon: `koine-domain`,
  `koine-application`, `koine-store-postgres`, and (phase 2A) `koine-grpc`,
  wired in by `serve.rs`'s authenticated data-plane `serve` command.
  `koine-store-memory`, `koine-http`, `koine-mcp`, and `koine-observability`
  were declared in `Cargo.toml` from the phase-0 workspace wiring but never
  referenced from this crate's source; a `cargo-machete` CI job now catches
  exactly that asymmetry (neither `cargo deny` nor clippy flag unused
  *dependencies*, only unused imports), so they were pruned rather than left
  inert — see `phase-2-carryover-hardening` AC3. Each rejoins `Cargo.toml`
  only once the phase that gives it real behavior (2B–4) wires it in here.
- Grows with each phase: the REST control plane and dashboard arrive in
  phase 3, the MCP adapter in phase 4 — each is wired into this same
  composition root, not a new one.
- `dev-loop` remains a development/exercise command, not a production entry
  point; `serve` (phase 2A) is the real production entry point that actually
  serves worker traffic — but only the data plane. `serve` has no unit tests
  of its own beyond `serve.rs`'s `parse_config` environment-parsing table.
  That table covers token/address/resource parsing and includes
  `zero_resource_values_are_rejected`, `invalid_pool_size_is_rejected`, and
  `invalid_acquire_timeout_is_rejected`; its transactional and transport
  behavior is exercised through `koine-grpc`'s test suites (which build the
  same `Deps` shape directly) rather than a `serve`-specific integration test.
- The configured operational pool contains at most `N` connections; the
  shared `PgSignal` listener is separate, making the process budget exactly
  `N + 1`. The listener fans one `LISTEN` subscription to every idle Fetch
  wait and does not consume operational capacity. `PgPresence` uses that
  operational pool best-effort: saturation skips the write immediately and
  never waits for the general acquisition timeout. After immediate
  acquisition, its synchronous write can add up to the 100 ms budget.
  Phase 3 must review this capacity budget before adding concurrent relay or
  `EventSink` consumers to the operational pool.
