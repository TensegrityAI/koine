//! Ring-2 lifecycle tests: use cases against the complete in-memory
//! adapters (testing-policy ring 2 — fast, no Docker).
#![allow(clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use koine_application::use_cases::cancel::CancelJob;
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::worker_ack::{AckOutcome, WorkerAck};
use koine_application::{Lineage, ports::Dispatcher as _, ports::EventStore as _};
use koine_domain::{JobError, JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, InMemoryDispatcher, InMemoryEventStore, SeededIds};

struct World {
    store: Arc<InMemoryEventStore>,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    dispatcher: InMemoryDispatcher<SeededIds, FixedClock>,
    queue: QueueName,
    worker: WorkerId,
}

fn world() -> World {
    let store = Arc::new(InMemoryEventStore::new());
    let ids = Arc::new(SeededIds::new(11));
    let clock = Arc::new(FixedClock::at(
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts"),
    ));
    let dispatcher =
        InMemoryDispatcher::new(Arc::clone(&store), Arc::clone(&ids), Arc::clone(&clock));
    World {
        store,
        ids,
        clock,
        dispatcher,
        queue: QueueName::new("default").expect("q"),
        worker: WorkerId::new("w1").expect("w"),
    }
}

fn tight_policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(2),
    }
}

async fn enqueue(w: &World, policy: RetryPolicy) -> JobId {
    EnqueueJob {
        store: w.store.as_ref(),
        ids: w.ids.as_ref(),
        clock: w.clock.as_ref(),
    }
    .execute(EnqueueCommand {
        queue: w.queue.clone(),
        payload: serde_json::json!({"work": true}),
        priority: Priority(0),
        retry_policy: policy,
        not_before: None,
        lineage: Lineage::default(),
    })
    .await
    .expect("enqueue")
}

async fn kinds(w: &World, job: JobId) -> Vec<&'static str> {
    w.store
        .load(job)
        .await
        .expect("load")
        .iter()
        .map(|env| env.event.kind())
        .collect()
}

fn ack(w: &World) -> WorkerAck<'_, InMemoryEventStore, SeededIds, FixedClock> {
    WorkerAck {
        store: w.store.as_ref(),
        ids: w.ids.as_ref(),
        clock: w.clock.as_ref(),
    }
}

#[tokio::test]
async fn happy_path_records_the_full_story() {
    let w = world();
    let job = enqueue(&w, RetryPolicy::default()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    assert_eq!(leased.job_id, job);
    assert_eq!(leased.attempt, 0);

    ack(&w).start(job, &w.worker).await.expect("start");
    let outcome = ack(&w)
        .succeed(
            job,
            &w.worker,
            leased.lease,
            Some(serde_json::json!("done")),
        )
        .await
        .expect("succeed");
    assert_eq!(outcome, AckOutcome::Recorded);
    assert_eq!(
        kinds(&w, job).await,
        vec!["enqueued", "leased", "started", "succeeded"]
    );
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none(),
        "terminal job must leave the dispatch index"
    );
}

#[tokio::test]
async fn retryable_failure_backs_off_then_retries() {
    let w = world();
    let job = enqueue(&w, tight_policy()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    ack(&w).start(job, &w.worker).await.expect("start");
    let outcome = ack(&w)
        .fail(
            job,
            &w.worker,
            leased.lease,
            JobError {
                kind: "io".into(),
                message: "boom".into(),
                stacktrace: None,
                retryable: true,
            },
        )
        .await
        .expect("fail");
    assert_eq!(outcome, AckOutcome::Recorded);
    assert_eq!(
        kinds(&w, job).await,
        vec!["enqueued", "leased", "started", "failed", "retry_scheduled"]
    );
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none(),
        "backoff must gate the retry"
    );
    w.clock.advance(Duration::from_secs(3));
    let retried = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("retry after backoff");
    assert_eq!(retried.attempt, 1);
}

#[tokio::test]
async fn non_retryable_failure_parks_immediately() {
    let w = world();
    let job = enqueue(&w, RetryPolicy::default()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    ack(&w).start(job, &w.worker).await.expect("start");
    ack(&w)
        .fail(
            job,
            &w.worker,
            leased.lease,
            JobError {
                kind: "bug".into(),
                message: "bad input".into(),
                stacktrace: None,
                retryable: false,
            },
        )
        .await
        .expect("fail");
    assert_eq!(kinds(&w, job).await.last(), Some(&"parked"));
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none()
    );
}

#[tokio::test]
async fn cancel_removes_a_pending_job() {
    let w = world();
    let job = enqueue(&w, RetryPolicy::default()).await;
    CancelJob {
        store: w.store.as_ref(),
        ids: w.ids.as_ref(),
        clock: w.clock.as_ref(),
    }
    .execute(job, Some("operator".into()))
    .await
    .expect("cancel");
    assert_eq!(kinds(&w, job).await, vec!["enqueued", "cancelled"]);
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none()
    );
}
