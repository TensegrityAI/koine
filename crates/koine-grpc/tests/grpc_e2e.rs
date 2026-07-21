//! Ring-3 gRPC e2e: the crash-recovery story over a real socket and a real
//! Postgres store. Unlike `wire.rs` (in-process duplex transport, in-memory
//! adapters), every test here binds a real `tonic` server to a real TCP
//! port and drives it against `koine-store-postgres`'s Postgres adapters —
//! so a broken TCP/HTTP2 transport setup, or a broken interaction between
//! the gRPC surface and the real store/dispatcher, would show up here even
//! though it can't in `wire.rs`.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use koine_application::Lineage;
use koine_application::ports::EventStore as _;
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::sweep::SweepExpiredLeases;
use koine_domain::{JobId, Priority, QueueName, RetryPolicy};
use koine_grpc::{Deps, GrpcConfig};
use koine_proto::v1;
use koine_proto::v1::worker_service_client::WorkerServiceClient;
use koine_store_postgres::{
    PgPresence, PgSignal, PoolConfig, PostgresDispatcher, PostgresEventStore,
};
use serde_json::Value;
use sqlx::PgPool;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Request;
use tonic::transport::{Channel, Endpoint, Server};

use support::{SystemClock, UuidV7Ids};

/// The bearer token every test client presents (per the brief).
const TOKEN: &str = "e2e-token";

fn payload() -> Value {
    serde_json::json!({"work": "grpc-e2e"})
}

/// A retry policy whose backoff jitter is always zero: the domain's full
/// jitter formula (spec §3) scales with `base_delay`, and the default
/// policy's `base_delay` of 2s would give `retry_scheduled` a `not_before`
/// anywhere up to 2s in the future — turning client B's redelivery wait
/// into an unbounded-feeling, non-deterministic delay. Zeroing `base_delay`
/// guarantees `not_before == now`, so the job is claimable the instant the
/// sweep reclaims it.
fn instant_retry_policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 20,
        base_delay: Duration::ZERO,
        max_delay: Duration::from_secs(1),
    }
}

/// Enqueues one job directly through the use case (there is no `Enqueue`
/// RPC on `WorkerService` — jobs enter the log out-of-band), against its own
/// short-lived store/id/clock instances (all stateless or Postgres-backed,
/// so no sharing is required the way `wire.rs`'s in-memory `Shared*`
/// newtypes need).
async fn enqueue(pool: &PgPool, queue: &QueueName, payload: Value) -> JobId {
    let store = PostgresEventStore::new(pool.clone());
    EnqueueJob {
        store: &store,
        ids: &UuidV7Ids,
        clock: &SystemClock,
    }
    .execute(EnqueueCommand {
        queue: queue.clone(),
        payload,
        priority: Priority(0),
        retry_policy: instant_retry_policy(),
        not_before: None,
        lineage: Lineage::default(),
    })
    .await
    .expect("enqueue")
}

/// Drives `SweepExpiredLeases` in a bounded retry loop until it reports at
/// least `min_swept` reclaimed leases in total, or `budget` elapses —
/// deliberately not a single fixed sleep followed by one sweep call: a
/// retry loop tolerates scheduler jitter around the exact TTL boundary
/// (real `SystemClock` time, not a `FixedClock`) instead of racing a single
/// point-in-time check.
async fn sweep_until(pool: &PgPool, min_swept: u32, budget: Duration) -> u32 {
    let dispatcher =
        PostgresDispatcher::new(pool.clone(), Arc::new(UuidV7Ids), Arc::new(SystemClock));
    let sweeper = SweepExpiredLeases {
        dispatcher: &dispatcher,
    };

    let deadline = Instant::now() + budget;
    let mut total = 0;
    loop {
        total += sweeper.execute().await.expect("sweep");
        if total >= min_swept || Instant::now() >= deadline {
            return total;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// The event-kind story recorded for `job`, in version order.
async fn load_kinds(pool: &PgPool, job: JobId) -> Vec<&'static str> {
    let store = PostgresEventStore::new(pool.clone());
    store
        .load(job)
        .await
        .expect("load")
        .iter()
        .map(|env| env.event.kind())
        .collect()
}

/// Spawns `koine_grpc::server(deps)` behind a real `tonic` transport bound
/// to an ephemeral TCP port on the loopback interface, wired onto real
/// Postgres adapters (`PgSignal`/`PgPresence`/`PostgresEventStore`/
/// `PostgresDispatcher`) plus the real `SystemClock`/`UuidV7Ids` runtime
/// types. Returns the bound address; the server task is detached (it dies
/// with the test process).
async fn spawn_server(
    database_url: &str,
    pool: PgPool,
    idle_poll: Duration,
) -> std::net::SocketAddr {
    let store = PostgresEventStore::new(pool.clone());
    let dispatcher =
        PostgresDispatcher::new(pool.clone(), Arc::new(UuidV7Ids), Arc::new(SystemClock));
    let signal = PgSignal::connect(
        database_url,
        pool.clone(),
        PoolConfig::default().acquire_timeout(),
    )
    .await
    .expect("connect listener");
    let presence = PgPresence::new(pool);

    let deps = Arc::new(Deps {
        store,
        dispatcher,
        ids: UuidV7Ids,
        clock: SystemClock,
        signal,
        presence,
        config: GrpcConfig {
            token: TOKEN.to_string(),
            max_lease_ttl: Duration::from_hours(1),
            idle_poll,
        },
    });

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind loopback ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    tokio::spawn(async move {
        let incoming = TcpListenerStream::new(listener);
        let _ = Server::builder()
            .add_service(koine_grpc::server(deps))
            .serve_with_incoming(incoming)
            .await;
    });

    addr
}

/// Dials a real client channel over TCP to the server bound at `addr`.
async fn connect(addr: std::net::SocketAddr) -> WorkerServiceClient<Channel> {
    let channel = Endpoint::from_shared(format!("http://{addr}"))
        .expect("valid endpoint uri")
        .connect()
        .await
        .expect("connect over tcp");
    WorkerServiceClient::new(channel)
}

/// Attaches the `authorization` + `koine-worker-id` metadata every
/// authenticated RPC needs, for the given worker identity (crash recovery
/// needs two distinct worker ids on one client, unlike `wire.rs`'s single
/// hardcoded `WORKER`).
fn authed<T>(worker: &str, message: T) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {TOKEN}")
            .parse()
            .expect("ascii metadata value"),
    );
    request.metadata_mut().insert(
        "koine-worker-id",
        worker.parse().expect("ascii metadata value"),
    );
    request
}

/// The `last_seen` timestamp recorded for `worker_id` in the presence
/// table, if any — queries `event_store.workers` directly (there is no
/// presence-read RPC).
async fn worker_last_seen(pool: &PgPool, worker_id: &str) -> Option<DateTime<Utc>> {
    let row: Option<(DateTime<Utc>,)> =
        sqlx::query_as("SELECT last_seen FROM event_store.workers WHERE worker_id = $1")
            .bind(worker_id)
            .fetch_optional(pool)
            .await
            .expect("query workers table");
    row.map(|(ts,)| ts)
}

#[tokio::test]
async fn crash_recovery_over_the_wire() {
    let (_guard, database_url, pool) = support::pg().await;
    let addr = spawn_server(&database_url, pool.clone(), Duration::from_millis(200)).await;
    let mut client = connect(addr).await;

    let queue = QueueName::new("default").expect("queue name");
    let job_id = enqueue(&pool, &queue, payload()).await;

    // Client A fetches with a 2s TTL, gets the job, then "crashes": drops
    // the stream and never acks.
    let mut stream_a = client
        .fetch(authed(
            "worker-a",
            v1::FetchRequest {
                queue: queue.to_string(),
                lease_ttl_ms: 2_000,
            },
        ))
        .await
        .expect("fetch stream opens")
        .into_inner();
    let job_a = stream_a
        .message()
        .await
        .expect("stream item")
        .expect("job available");
    assert_eq!(job_a.job_id, job_id.to_string());
    assert_eq!(job_a.attempt, 0, "first delivery is attempt 0");

    drop(stream_a); // simulated crash: never starts, never acks

    // Genuinely wait past the 2s ttl (SystemClock is wall-clock, not a
    // FixedClock we can fast-forward) and drive the sweep directly — the
    // server's own sweep ticker isn't running in this harness (only the
    // tonic server is spawned, not serve.rs's background tickers).
    let swept = sweep_until(&pool, 1, Duration::from_secs(15)).await;
    assert!(
        swept >= 1,
        "sweep must reclaim the expired lease, got {swept}"
    );

    // Client B fetches: the SAME job, now at attempt == 1.
    let mut stream_b = client
        .fetch(authed(
            "worker-b",
            v1::FetchRequest {
                queue: queue.to_string(),
                lease_ttl_ms: 30_000,
            },
        ))
        .await
        .expect("fetch stream opens")
        .into_inner();
    let job_b = stream_b
        .message()
        .await
        .expect("stream item")
        .expect("job available");
    assert_eq!(job_b.job_id, job_id.to_string(), "same job redelivered");
    assert_eq!(job_b.attempt, 1, "second delivery is attempt 1");

    // B starts and succeeds.
    client
        .start(authed(
            "worker-b",
            v1::StartRequest {
                job_id: job_b.job_id.clone(),
            },
        ))
        .await
        .expect("start over the wire");
    let ack_b = client
        .succeed(authed(
            "worker-b",
            v1::SucceedRequest {
                job_id: job_b.job_id.clone(),
                lease_id: job_b.lease_id.clone(),
                result_json: Some(serde_json::to_string(&serde_json::json!("done")).expect("json")),
            },
        ))
        .await
        .expect("succeed over the wire")
        .into_inner();
    assert_eq!(ack_b.outcome, v1::AckOutcome::Recorded as i32);

    // The exact crash-recovery arc, loaded back from the real store.
    assert_eq!(
        load_kinds(&pool, job_id).await,
        vec![
            "enqueued",
            "leased",
            "lease_expired",
            "retry_scheduled",
            "leased",
            "started",
            "succeeded",
        ]
    );

    // A's late succeed, against its now-stale lease, must be recorded as a
    // conflict — never silently dropped (spec §3: information is never
    // lost) and never a transport-level error.
    let ack_a = client
        .succeed(authed(
            "worker-a",
            v1::SucceedRequest {
                job_id: job_a.job_id.clone(),
                lease_id: job_a.lease_id.clone(),
                result_json: None,
            },
        ))
        .await
        .expect("stale succeed still returns a response, not an error")
        .into_inner();
    assert_eq!(ack_a.outcome, v1::AckOutcome::Conflict as i32);

    let mut kinds = load_kinds(&pool, job_id).await;
    assert_eq!(
        kinds.pop(),
        Some("late_ack_conflict"),
        "the stale ack must be recorded, never silently dropped"
    );
}

#[tokio::test]
async fn presence_rows_appear() {
    // Its own container, per the ring-3 "one throwaway Postgres container
    // per test" convention (see support/mod.rs) — presence rows have no
    // dependency on the crash-recovery arc, so a fresh, independent
    // fixture is simpler and less flaky than sequencing on shared state.
    let (_guard, database_url, pool) = support::pg().await;
    let addr = spawn_server(&database_url, pool.clone(), Duration::from_millis(200)).await;
    let mut client = connect(addr).await;
    let queue = QueueName::new("default").expect("queue name");

    // `WorkerApi::fetch` records presence synchronously before it spawns
    // its poll loop (see `service.rs`), so merely opening a fetch stream —
    // even against an empty queue — is enough to register the worker; no
    // job needs to exist for this assertion.
    let _stream_a = client
        .fetch(authed(
            "worker-a",
            v1::FetchRequest {
                queue: queue.to_string(),
                lease_ttl_ms: 30_000,
            },
        ))
        .await
        .expect("fetch stream opens")
        .into_inner();
    let _stream_b = client
        .fetch(authed(
            "worker-b",
            v1::FetchRequest {
                queue: queue.to_string(),
                lease_ttl_ms: 30_000,
            },
        ))
        .await
        .expect("fetch stream opens")
        .into_inner();

    let seen_a = worker_last_seen(&pool, "worker-a")
        .await
        .expect("worker-a has a presence row");
    let seen_b = worker_last_seen(&pool, "worker-b")
        .await
        .expect("worker-b has a presence row");

    let now = Utc::now();
    for (worker, last_seen) in [("worker-a", seen_a), ("worker-b", seen_b)] {
        let age = (now - last_seen).num_seconds().abs();
        assert!(
            age < 60,
            "{worker}'s last_seen must be recent (within 60s), age={age}s"
        );
    }
}
