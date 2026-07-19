//! Wire suite: drives the real tonic transport (client → gRPC → service)
//! over an in-process duplex connection, backed by the in-memory adapters.
//! Unlike `fetch_idle_disconnect.rs` (which calls `WorkerApi::fetch`
//! directly as a trait method), every test here goes through a generated
//! `WorkerServiceClient` talking to a `tonic::transport::Server` — so a
//! broken (de)serialization, routing, or metadata-plumbing bug would show up
//! here even though it can't in a direct trait-method test.
#![allow(clippy::expect_used)]

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use hyper_util::rt::TokioIo;
use koine_application::Lineage;
use koine_application::ports::{
    Clock, DispatchSignal as _, EventStore, EventStoreError, IdGenerator,
};
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::sweep::SweepExpiredLeases;
use koine_domain::{
    CorrelationId, EventEnvelope, EventId, JobId, LeaseId, Priority, QueueName, RetryPolicy,
};
use koine_grpc::{Deps, GrpcConfig};
use koine_proto::v1;
use koine_proto::v1::worker_service_client::WorkerServiceClient;
use koine_store_memory::{
    FixedClock, InMemoryDispatcher, InMemoryEventStore, NoopPresence, NotifySignal, SeededIds,
};
use serde_json::Value;
use tonic::transport::{Channel, Endpoint, Server, Uri};
use tonic::{Code, Request};
use tower::service_fn;
use uuid::Uuid;

/// The bearer token every test client presents (mirrors the "test-token"
/// convention from `fetch_idle_disconnect.rs`).
const TOKEN: &str = "test-token";
/// The worker identity every test client presents.
const WORKER: &str = "worker-1";

/// Thin newtype forwarding `EventStore` to a shared `Arc<InMemoryEventStore>`
/// so the same store instance backs both the gRPC service's `Deps` and the
/// test's direct use-case calls (enqueue, sweep) — required because
/// `InMemoryEventStore` isn't `Clone` and `Deps` holds its ports by value.
struct SharedStore(Arc<InMemoryEventStore>);

impl EventStore for SharedStore {
    fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> impl Future<Output = Result<(), EventStoreError>> + Send {
        self.0.append(stream, expected_version, envelopes)
    }

    fn load(
        &self,
        stream: JobId,
    ) -> impl Future<Output = Result<Vec<EventEnvelope>, EventStoreError>> + Send {
        self.0.load(stream)
    }
}

/// Thin newtype forwarding `Clock` to a shared `Arc<FixedClock>` so tests can
/// advance the same clock instance the gRPC service reads from.
struct SharedClock(Arc<FixedClock>);

impl Clock for SharedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0.now()
    }
}

/// Thin newtype forwarding `IdGenerator` to a shared `Arc<SeededIds>` so ids
/// minted by the service and by the test's direct use-case calls come from
/// one sequence.
struct SharedIds(Arc<SeededIds>);

impl IdGenerator for SharedIds {
    fn job_id(&self) -> JobId {
        self.0.job_id()
    }
    fn event_id(&self) -> EventId {
        self.0.event_id()
    }
    fn lease_id(&self) -> LeaseId {
        self.0.lease_id()
    }
    fn correlation_id(&self) -> CorrelationId {
        self.0.correlation_id()
    }
    fn jitter_seed(&self) -> u64 {
        self.0.jitter_seed()
    }
}

/// The concrete `Deps` instantiation the test harness wires up.
type Dep = Deps<
    SharedStore,
    InMemoryDispatcher<SeededIds, FixedClock>,
    SharedIds,
    SharedClock,
    NotifySignal,
    NoopPresence,
>;

/// Everything a test needs to drive the store/dispatcher directly (enqueue,
/// sweep, clock advance) alongside the wire client — mirrors the ring-2
/// `World` pattern (`koine-store-memory/tests/lifecycle.rs`), plus the
/// shared `Deps` handle the service runs on.
struct TestWorld {
    deps: Arc<Dep>,
    store: Arc<InMemoryEventStore>,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    queue: QueueName,
}

impl TestWorld {
    /// Enqueues a job directly through the use case (there is no `Enqueue`
    /// RPC on `WorkerService` — jobs enter the log out-of-band).
    async fn enqueue(&self, payload: Value) -> JobId {
        EnqueueJob {
            store: self.store.as_ref(),
            ids: self.ids.as_ref(),
            clock: self.clock.as_ref(),
        }
        .execute(EnqueueCommand {
            queue: self.queue.clone(),
            payload,
            priority: Priority(0),
            retry_policy: RetryPolicy::default(),
            not_before: None,
            lineage: Lineage::default(),
        })
        .await
        .expect("enqueue")
    }

    /// The event-kind story recorded for `job`, in version order.
    async fn kinds(&self, job: JobId) -> Vec<&'static str> {
        self.store
            .load(job)
            .await
            .expect("load")
            .iter()
            .map(|env| env.event.kind())
            .collect()
    }

    /// A `SweepExpiredLeases` use case constructed directly on this world's
    /// store/dispatcher/ids/clock.
    fn sweeper(
        &self,
    ) -> SweepExpiredLeases<
        '_,
        InMemoryEventStore,
        InMemoryDispatcher<SeededIds, FixedClock>,
        SeededIds,
        FixedClock,
    > {
        SweepExpiredLeases {
            store: self.store.as_ref(),
            dispatcher: &self.deps.dispatcher,
            ids: self.ids.as_ref(),
            clock: self.clock.as_ref(),
        }
    }
}

/// Attaches the `authorization` + `koine-worker-id` metadata every
/// authenticated RPC needs.
fn authed<T>(message: T) -> Request<T> {
    let mut request = Request::new(message);
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {TOKEN}")
            .parse()
            .expect("ascii metadata value"),
    );
    request.metadata_mut().insert(
        "koine-worker-id",
        WORKER.parse().expect("ascii metadata value"),
    );
    request
}

/// Spawns `koine_grpc::server(deps)` behind a real tonic transport, wired
/// over an in-process `tokio::io::duplex` pair (no TCP port bound) — the
/// tonic 0.14 pattern: the server side accepts the raw `DuplexStream`
/// directly (it already implements `Connected`), the client side wraps its
/// end in `hyper_util::rt::TokioIo` (so it satisfies `hyper::rt::Read`/
/// `Write`) and dials it through a `tower::service_fn` connector.
async fn connect(deps: Arc<Dep>) -> WorkerServiceClient<Channel> {
    let (client_io, server_io) = tokio::io::duplex(64 * 1024);

    tokio::spawn(async move {
        let incoming = tokio_stream::once(Ok::<_, std::io::Error>(server_io));
        let _ = Server::builder()
            .add_service(koine_grpc::server(deps))
            .serve_with_incoming(incoming)
            .await;
    });

    let mut client_io = Some(client_io);
    let channel = Endpoint::from_static("http://[::]:50051")
        .connect_with_connector(service_fn(move |_: Uri| {
            let io = client_io.take();
            async move {
                io.map(TokioIo::new)
                    .ok_or_else(|| std::io::Error::other("duplex client end already taken"))
            }
        }))
        .await
        .expect("connect over the in-process duplex transport");

    WorkerServiceClient::new(channel)
}

/// Builds the harness with a caller-chosen `idle_poll` (test 5 needs a long
/// one to prove the signal path, not the idle-poll fallback, wakes it).
async fn spawn_server_with_idle_poll(
    idle_poll: Duration,
) -> (WorkerServiceClient<Channel>, TestWorld) {
    let store = Arc::new(InMemoryEventStore::new());
    let ids = Arc::new(SeededIds::new(1));
    let clock = Arc::new(FixedClock::at(Utc::now()));
    let dispatcher =
        InMemoryDispatcher::new(Arc::clone(&store), Arc::clone(&ids), Arc::clone(&clock));

    let deps = Arc::new(Deps {
        store: SharedStore(Arc::clone(&store)),
        dispatcher,
        ids: SharedIds(Arc::clone(&ids)),
        clock: SharedClock(Arc::clone(&clock)),
        signal: NotifySignal::new(),
        presence: NoopPresence,
        config: GrpcConfig {
            token: TOKEN.to_string(),
            max_lease_ttl: Duration::from_hours(1),
            idle_poll,
        },
    });

    let client = connect(Arc::clone(&deps)).await;
    let world = TestWorld {
        deps,
        store,
        ids,
        clock,
        queue: QueueName::new("default").expect("queue name"),
    };
    (client, world)
}

/// The harness with a short `idle_poll` — the default for every test that
/// isn't specifically proving the signal-vs-idle-poll distinction.
async fn spawn_server() -> (WorkerServiceClient<Channel>, TestWorld) {
    spawn_server_with_idle_poll(Duration::from_millis(50)).await
}

fn payload() -> Value {
    serde_json::json!({"work": "wire"})
}

#[tokio::test]
async fn unauthenticated_calls_are_rejected() {
    let (mut client, _world) = spawn_server().await;

    // No metadata at all.
    let err = client
        .start(Request::new(v1::StartRequest {
            job_id: Uuid::nil().to_string(),
        }))
        .await
        .expect_err("missing credentials must be rejected");
    assert_eq!(err.code(), Code::Unauthenticated);

    // Wrong token, but the same length as the real one — exercises the
    // constant-time comparison path rather than a length short-circuit.
    let wrong_token = "x".repeat(TOKEN.len());
    assert_ne!(wrong_token, TOKEN);
    let mut request = Request::new(v1::StartRequest {
        job_id: Uuid::nil().to_string(),
    });
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {wrong_token}")
            .parse()
            .expect("ascii metadata value"),
    );
    request.metadata_mut().insert(
        "koine-worker-id",
        WORKER.parse().expect("ascii metadata value"),
    );
    let err = client
        .start(request)
        .await
        .expect_err("wrong same-length token must be rejected");
    assert_eq!(err.code(), Code::Unauthenticated);
}

#[tokio::test]
async fn fetch_streams_a_claimed_job() {
    let (mut client, world) = spawn_server().await;
    let payload = payload();
    let job_id = world.enqueue(payload.clone()).await;

    let mut stream = client
        .fetch(authed(v1::FetchRequest {
            queue: world.queue.to_string(),
            lease_ttl_ms: 30_000,
        }))
        .await
        .expect("fetch stream opens")
        .into_inner();

    let job = stream
        .message()
        .await
        .expect("stream item")
        .expect("job available");

    assert_eq!(job.job_id, job_id.to_string());
    let round_tripped: Value = serde_json::from_str(&job.payload_json).expect("payload json");
    assert_eq!(round_tripped, payload, "payload round-trips over the wire");
    assert_eq!(job.attempt, 0);
    Uuid::parse_str(&job.job_id).expect("job_id is a UUID");
    Uuid::parse_str(&job.lease_id).expect("lease_id is a UUID");
    Uuid::parse_str(&job.correlation_id).expect("correlation_id is a UUID");
}

#[tokio::test]
async fn full_story_over_the_wire() {
    let (mut client, world) = spawn_server().await;
    let job_id = world.enqueue(payload()).await;

    let mut stream = client
        .fetch(authed(v1::FetchRequest {
            queue: world.queue.to_string(),
            lease_ttl_ms: 30_000,
        }))
        .await
        .expect("fetch stream opens")
        .into_inner();
    let job = stream
        .message()
        .await
        .expect("stream item")
        .expect("job available");

    client
        .start(authed(v1::StartRequest {
            job_id: job.job_id.clone(),
        }))
        .await
        .expect("start over the wire");

    let ack = client
        .succeed(authed(v1::SucceedRequest {
            job_id: job.job_id.clone(),
            lease_id: job.lease_id.clone(),
            result_json: Some(serde_json::to_string(&serde_json::json!("done")).expect("json")),
        }))
        .await
        .expect("succeed over the wire")
        .into_inner();

    assert_eq!(ack.outcome, v1::AckOutcome::Recorded as i32);
    assert_eq!(
        world.kinds(job_id).await,
        vec!["enqueued", "leased", "started", "succeeded"]
    );
}

#[tokio::test]
async fn stale_ack_returns_conflict() {
    let (mut client, world) = spawn_server().await;
    let job_id = world.enqueue(payload()).await;

    let mut stream = client
        .fetch(authed(v1::FetchRequest {
            queue: world.queue.to_string(),
            lease_ttl_ms: 30_000,
        }))
        .await
        .expect("fetch stream opens")
        .into_inner();
    let job = stream
        .message()
        .await
        .expect("stream item")
        .expect("job available");

    // The lease outlives its ttl and the sweep reclaims it before the
    // worker's ack arrives — the sweep is driven directly on the world, not
    // via a ticker (there is no periodic sweep RPC).
    world.clock.advance(Duration::from_secs(31));
    world.sweeper().execute().await.expect("sweep");

    let ack = client
        .succeed(authed(v1::SucceedRequest {
            job_id: job.job_id.clone(),
            lease_id: job.lease_id.clone(),
            result_json: None,
        }))
        .await
        .expect("stale succeed still returns a response, not an error")
        .into_inner();

    assert_eq!(ack.outcome, v1::AckOutcome::Conflict as i32);
    assert!(
        world.kinds(job_id).await.contains(&"late_ack_conflict"),
        "the stale ack must be recorded, never silently dropped"
    );
}

#[tokio::test]
async fn fetch_wakes_on_late_enqueue() {
    // A long idle_poll proves that any wakeup faster than it came from the
    // dispatch signal, not the idle-poll fallback re-check.
    let (mut client, world) = spawn_server_with_idle_poll(Duration::from_secs(10)).await;

    let mut stream = client
        .fetch(authed(v1::FetchRequest {
            queue: world.queue.to_string(),
            lease_ttl_ms: 30_000,
        }))
        .await
        .expect("fetch stream opens on an empty queue")
        .into_inner();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let job_id = world.enqueue(payload()).await;
        // The in-memory store's `append` does not itself call
        // `DispatchSignal::notify` (unlike the Postgres adapter's append,
        // which issues `pg_notify` in the same statement — see
        // koine-store-postgres/src/store.rs). So the test must signal the
        // queue itself after enqueueing, exactly mirroring what production
        // does on the Postgres path.
        world.deps.signal.notify(&world.queue).await;
        let _ = job_id;
    });

    let job = tokio::time::timeout(Duration::from_secs(1), stream.message())
        .await
        .expect("fetch must yield well within 1s of the signal, not wait for the 10s idle_poll")
        .expect("stream item")
        .expect("job available");

    Uuid::parse_str(&job.job_id).expect("job_id is a UUID");
}

#[tokio::test]
async fn heartbeat_reports_liveness() {
    let (mut client, world) = spawn_server().await;
    let _job_id = world.enqueue(payload()).await;

    let mut stream = client
        .fetch(authed(v1::FetchRequest {
            queue: world.queue.to_string(),
            lease_ttl_ms: 30_000,
        }))
        .await
        .expect("fetch stream opens")
        .into_inner();
    let job = stream
        .message()
        .await
        .expect("stream item")
        .expect("job available");

    let alive = client
        .heartbeat(authed(v1::HeartbeatRequest {
            lease_id: job.lease_id.clone(),
            ttl_ms: 10_000,
        }))
        .await
        .expect("heartbeat over the wire")
        .into_inner()
        .alive;
    assert!(alive, "a freshly (re)extended lease must report alive");

    world.clock.advance(Duration::from_secs(11));

    let alive = client
        .heartbeat(authed(v1::HeartbeatRequest {
            lease_id: job.lease_id.clone(),
            ttl_ms: 10_000,
        }))
        .await
        .expect("heartbeat over the wire")
        .into_inner()
        .alive;
    assert!(!alive, "a lease past its ttl must report dead");
}
