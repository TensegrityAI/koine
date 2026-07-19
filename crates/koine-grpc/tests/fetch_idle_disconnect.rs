//! Regression test (Task 5 review, Fix 2): a `Fetch` client that disconnects
//! while its queue is idle must not leave the spawned polling task running
//! forever. Before the fix, the idle arm only awaited the dispatch signal,
//! so a dropped receiver was never noticed and the task kept calling
//! `lease_next` on a timer indefinitely (a resource leak).
//!
//! This drives `WorkerApi::fetch` directly (the generated `WorkerService`
//! trait method) against the in-memory adapters, wraps the `Dispatcher` in a
//! counting layer, drops the response stream while the queue is empty, and
//! asserts the `lease_next` call count plateaus shortly after — proving the
//! spawned task actually exits instead of continuing to poll.

use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use koine_application::ports::{DispatchError, Dispatcher, LeasedJob};
use koine_domain::{JobId, LeaseId, QueueName, WorkerId};
use koine_grpc::{Deps, GrpcConfig, WorkerApi};
use koine_proto::v1;
use koine_proto::v1::worker_service_server::WorkerService as _;
use koine_store_memory::{
    FixedClock, InMemoryDispatcher, InMemoryEventStore, NoopPresence, NotifySignal, SeededIds,
};
use tonic::Request;

/// Wraps a `Dispatcher`, counting every `lease_next` call so the test can
/// tell whether the fetch loop is still polling after the stream is dropped.
struct CountingDispatcher<D> {
    inner: D,
    lease_next_calls: Arc<AtomicUsize>,
}

impl<D: Dispatcher> Dispatcher for CountingDispatcher<D> {
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send {
        self.lease_next_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.lease_next(queue, worker, ttl)
    }

    fn extend_lease(
        &self,
        lease: LeaseId,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool, DispatchError>> + Send {
        self.inner.extend_lease(lease, ttl)
    }

    fn expired(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<JobId>, DispatchError>> + Send {
        self.inner.expired(now)
    }
}

#[tokio::test]
async fn fetch_task_ends_when_receiver_drops_while_idle() {
    let dispatcher_store = Arc::new(InMemoryEventStore::new());
    let dispatcher_ids = Arc::new(SeededIds::new(1));
    let dispatcher_clock = Arc::new(FixedClock::at(Utc::now()));
    let inner_dispatcher = InMemoryDispatcher::new(
        Arc::clone(&dispatcher_store),
        Arc::clone(&dispatcher_ids),
        Arc::clone(&dispatcher_clock),
    );
    let lease_next_calls = Arc::new(AtomicUsize::new(0));
    let dispatcher = CountingDispatcher {
        inner: inner_dispatcher,
        lease_next_calls: Arc::clone(&lease_next_calls),
    };

    // Short idle_poll keeps the test fast; margins below are generous
    // multiples of it, so this stays deterministic without tight timing.
    let idle_poll = Duration::from_millis(50);
    let token = "test-token".to_string();
    let deps = Arc::new(Deps {
        store: InMemoryEventStore::new(),
        dispatcher,
        ids: SeededIds::new(2),
        clock: FixedClock::at(Utc::now()),
        signal: NotifySignal::new(),
        presence: NoopPresence,
        config: GrpcConfig {
            token: token.clone(),
            max_lease_ttl: Duration::from_secs(30),
            idle_poll,
        },
    });

    let api = WorkerApi::new(Arc::clone(&deps));

    let mut request = Request::new(v1::FetchRequest {
        queue: "default".to_string(),
        lease_ttl_ms: 30_000,
    });
    request.metadata_mut().insert(
        "authorization",
        format!("Bearer {token}")
            .parse()
            .expect("ascii metadata value"),
    );
    request.metadata_mut().insert(
        "koine-worker-id",
        "worker-1".parse().expect("ascii metadata value"),
    );

    let response = api.fetch(request).await.expect("fetch stream opens");
    let stream = response.into_inner();

    // Let the spawned loop poll the empty queue a few times before dropping
    // the stream, so we know it was actually running.
    tokio::time::sleep(idle_poll * 3).await;
    assert!(
        lease_next_calls.load(Ordering::SeqCst) > 0,
        "the idle loop must have polled the dispatcher at least once"
    );

    drop(stream);

    // Two idle-poll periods is a generous margin for the spawned task to
    // notice the closed receiver via `tx.closed()` and break out of the
    // loop instead of continuing to lease work nobody will read.
    tokio::time::sleep(idle_poll * 2).await;
    let calls_after_drop = lease_next_calls.load(Ordering::SeqCst);

    tokio::time::sleep(idle_poll * 2).await;
    let calls_after_more_waiting = lease_next_calls.load(Ordering::SeqCst);

    assert_eq!(
        calls_after_drop, calls_after_more_waiting,
        "lease_next call count must plateau once the receiver is dropped; \
         a still-increasing count means the spawned fetch task leaked"
    );
}
