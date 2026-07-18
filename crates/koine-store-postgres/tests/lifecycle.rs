//! Ring-3 lifecycle tests: the same crash-recovery story from
//! `koine-store-memory`'s ring-2 suite, replayed against the complete
//! Postgres adapters (testing-policy ring 3 — real SQL, via Docker).
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::sweep::SweepExpiredLeases;
use koine_application::use_cases::worker_ack::{AckOutcome, WorkerAck};
use koine_application::{Lineage, ports::Dispatcher as _, ports::EventStore as _};
use koine_domain::{JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::{PostgresDispatcher, PostgresEventStore};

struct World {
    _guard: testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
    store: PostgresEventStore,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    dispatcher: PostgresDispatcher<SeededIds, FixedClock>,
    queue: QueueName,
    worker: WorkerId,
}

async fn world() -> World {
    let (guard, pool) = support::pg().await;
    let ids = Arc::new(SeededIds::new(11));
    let clock = Arc::new(FixedClock::at(
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts"),
    ));
    let dispatcher = PostgresDispatcher::new(pool.clone(), Arc::clone(&ids), Arc::clone(&clock));
    World {
        _guard: guard,
        store: PostgresEventStore::new(pool),
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
        store: &w.store,
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

fn ack(w: &World) -> WorkerAck<'_, PostgresEventStore, SeededIds, FixedClock> {
    WorkerAck {
        store: &w.store,
        ids: w.ids.as_ref(),
        clock: w.clock.as_ref(),
    }
}

fn sweeper(
    w: &World,
) -> SweepExpiredLeases<
    '_,
    PostgresEventStore,
    PostgresDispatcher<SeededIds, FixedClock>,
    SeededIds,
    FixedClock,
> {
    SweepExpiredLeases {
        store: &w.store,
        dispatcher: &w.dispatcher,
        ids: w.ids.as_ref(),
        clock: w.clock.as_ref(),
    }
}

#[tokio::test]
async fn happy_path_records_the_full_story() {
    let w = world().await;
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
async fn worker_crash_is_recovered_by_the_sweep() {
    let w = world().await;
    let job = enqueue(&w, tight_policy()).await;
    let leased = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    // the worker "dies" here: no start, no ack, no heartbeat
    w.clock.advance(Duration::from_secs(31));
    assert_eq!(sweeper(&w).execute().await.expect("sweep"), 1);
    let story = kinds(&w, job).await;
    assert_eq!(
        story,
        vec!["enqueued", "leased", "lease_expired", "retry_scheduled"]
    );

    w.clock.advance(Duration::from_secs(3));
    let retried = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("recovered");
    assert_eq!(retried.job_id, job);
    assert_eq!(retried.attempt, 1, "crash counts as an attempt");
    let _ = leased;
}

#[tokio::test]
async fn late_ack_after_expiry_is_recorded_never_lost() {
    let w = world().await;
    let job = enqueue(&w, tight_policy()).await;
    let stale = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(31));
    sweeper(&w).execute().await.expect("sweep");

    // the presumed-dead worker comes back and acks with its stale lease
    let outcome = ack(&w)
        .succeed(job, &w.worker, stale.lease, None)
        .await
        .expect("late ack path");
    assert_eq!(outcome, AckOutcome::Conflict);
    assert_eq!(kinds(&w, job).await.last(), Some(&"late_ack_conflict"));

    // and the job's real lifecycle is untouched: it retries normally
    w.clock.advance(Duration::from_secs(3));
    let retried = w
        .dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("still claimable");
    ack(&w).start(job, &w.worker).await.expect("start");
    let outcome = ack(&w)
        .succeed(job, &w.worker, retried.lease, None)
        .await
        .expect("succeed");
    assert_eq!(outcome, AckOutcome::Recorded);
    assert_eq!(kinds(&w, job).await.last(), Some(&"succeeded"));
}

#[tokio::test]
#[allow(clippy::duration_suboptimal_units)]
async fn repeated_crashes_exhaust_into_parked() {
    let w = world().await;
    let policy = RetryPolicy {
        max_attempts: 1,
        ..tight_policy()
    };
    let job = enqueue(&w, policy).await;
    w.dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(31));
    sweeper(&w).execute().await.expect("sweep");
    assert_eq!(kinds(&w, job).await.last(), Some(&"parked"));
    w.clock.advance(Duration::from_secs(60));
    assert!(
        w.dispatcher
            .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .is_none(),
        "parked jobs await repair, not dispatch"
    );
}

#[tokio::test]
async fn sweep_surfaces_non_transition_domain_errors() {
    // A poisoned policy that folds fine but overflows chrono at decision time:
    // base/max near u64::MAX ms. Enqueue-side validation now blocks this at
    // the boundary, so construct the stream directly through the store to
    // simulate pre-validation data (or a future migration gap).
    use koine_application::ports::IdGenerator;
    let w = world().await;
    let stream = w.ids.job_id();
    let poisoned = RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::MAX,
        max_delay: Duration::MAX,
    };
    let event = koine_domain::Job::initial_event(
        w.queue.clone(),
        serde_json::json!({}),
        Priority(0),
        poisoned,
        None,
    );
    let envs = koine_application::wrap_events(
        w.ids.as_ref(),
        w.clock.as_ref(),
        stream,
        0,
        w.ids.correlation_id(),
        None,
        None,
        vec![event],
    );
    w.store
        .append(stream, 0, envs)
        .await
        .expect("direct append");
    w.dispatcher
        .lease_next(&w.queue, &w.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    w.clock.advance(Duration::from_secs(31));
    let err = sweeper(&w)
        .execute()
        .await
        .expect_err("InvalidTtl must surface");
    assert!(matches!(
        err,
        koine_application::use_cases::sweep::SweepError::Domain(_)
    ));
}
