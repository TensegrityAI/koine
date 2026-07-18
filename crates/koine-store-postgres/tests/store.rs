//! Ring-3 contract tests for the Postgres event store.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use koine_application::ports::{EventStore as _, IdGenerator as _};
use koine_application::wrap_events;
use koine_domain::{Job, JobEvent, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::PostgresEventStore;

fn clock() -> FixedClock {
    use chrono::TimeZone as _;
    FixedClock::at(
        chrono::Utc
            .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts"),
    )
}

fn enqueue_envelopes(
    ids: &SeededIds,
    clock: &FixedClock,
) -> (koine_domain::JobId, Vec<koine_domain::EventEnvelope>) {
    let stream = ids.job_id();
    let correlation = ids.correlation_id();
    let event = Job::initial_event(
        QueueName::new("default").expect("q"),
        serde_json::json!({"n": 1}),
        Priority(0),
        RetryPolicy::default(),
        None,
    );
    (
        stream,
        wrap_events(ids, clock, stream, 0, correlation, None, None, vec![event]),
    )
}

#[tokio::test]
async fn migrations_apply_cleanly() {
    let (_guard, pool) = support::pg().await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.events")
        .fetch_one(&pool)
        .await
        .expect("query");
    assert_eq!(n, 0);
}

#[tokio::test]
async fn appends_and_loads_round_trip() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool);
    let ids = SeededIds::new(21);
    let clock = clock();
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store
        .append(stream, 0, envelopes.clone())
        .await
        .expect("append");
    let loaded = store.load(stream).await.expect("load");
    assert_eq!(loaded, envelopes, "column round-trip must be lossless");
}

#[tokio::test]
async fn rejects_version_conflicts() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool);
    let ids = SeededIds::new(22);
    let clock = clock();
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store
        .append(stream, 0, envelopes.clone())
        .await
        .expect("append");
    let err = store
        .append(stream, 0, envelopes)
        .await
        .expect_err("conflict");
    assert!(matches!(
        err,
        koine_application::EventStoreError::VersionConflict { expected: 0, .. }
    ));
}

#[tokio::test]
async fn failed_append_leaves_no_trace_fresh_or_existing() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool);
    let ids = SeededIds::new(23);
    let clock = clock();

    // fresh stream, illegal opener
    let stream = ids.job_id();
    let bad = wrap_events(
        &ids,
        &clock,
        stream,
        0,
        ids.correlation_id(),
        None,
        None,
        vec![JobEvent::Suspended],
    );
    let err = store
        .append(stream, 0, bad)
        .await
        .expect_err("must not fold");
    assert!(matches!(
        err,
        koine_application::EventStoreError::Backend(_)
    ));
    assert!(matches!(
        store.load(stream).await.expect_err("no residue"),
        koine_application::EventStoreError::StreamNotFound(_)
    ));

    // existing stream, illegal continuation — prior events survive
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store
        .append(stream, 0, envelopes.clone())
        .await
        .expect("enqueue");
    let bad = wrap_events(
        &ids,
        &clock,
        stream,
        1,
        ids.correlation_id(),
        None,
        None,
        vec![JobEvent::Started {
            worker: WorkerId::new("w").expect("w"),
        }],
    );
    store
        .append(stream, 1, bad)
        .await
        .expect_err("must not fold");
    assert_eq!(store.load(stream).await.expect("intact"), envelopes);
}

#[tokio::test]
async fn append_maintains_dispatch_row_and_outbox() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool.clone());
    let ids = SeededIds::new(24);
    let clock = clock();
    let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
    store.append(stream, 0, envelopes).await.expect("append");

    let (queue, lease_id): (String, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT queue, lease_id FROM event_store.dispatch_queue WHERE job_id = $1")
            .bind(stream.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("dispatch row exists");
    assert_eq!(queue, "default");
    assert!(lease_id.is_none());

    let outbox: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.outbox")
        .fetch_one(&pool)
        .await
        .expect("outbox count");
    assert_eq!(outbox, 1, "enqueued event rides the outbox");

    // cancel ⇒ row removed, second outbox entry — same transaction contract
    let stream_envs = store.load(stream).await.expect("load");
    let job = Job::from_events(&stream_envs).expect("fold");
    let cancel = wrap_events(
        &ids,
        &clock,
        stream,
        job.version,
        ids.correlation_id(),
        None,
        None,
        vec![JobEvent::Cancelled { reason: None }],
    );
    store
        .append(stream, job.version, cancel)
        .await
        .expect("cancel");
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM event_store.dispatch_queue WHERE job_id = $1")
            .bind(stream.as_uuid())
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(rows, 0, "terminal ⇒ undispatchable");
}
