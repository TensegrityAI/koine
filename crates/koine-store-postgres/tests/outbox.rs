//! Ring-3 outbox relay tests: claim-delete semantics (ADR 0012).
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Mutex;

use koine_application::ports::{EventSink, EventStore as _, IdGenerator as _, SinkError};
use koine_application::wrap_events;
use koine_domain::{EventEnvelope, Job, Priority, QueueName, RetryPolicy};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::{PostgresEventStore, PostgresOutboxRelay};

struct Collecting(Mutex<Vec<String>>);
impl EventSink for Collecting {
    async fn deliver(&self, envelopes: &[EventEnvelope]) -> Result<(), SinkError> {
        let mut seen = self.0.lock().expect("lock");
        seen.extend(
            envelopes
                .iter()
                .map(|e| format!("{}:{}", e.stream_id, e.event.kind())),
        );
        Ok(())
    }
}

struct Failing;
impl EventSink for Failing {
    async fn deliver(&self, _: &[EventEnvelope]) -> Result<(), SinkError> {
        Err(SinkError::Failed("down".into()))
    }
}

fn clock() -> FixedClock {
    use chrono::TimeZone as _;
    FixedClock::at(
        chrono::Utc
            .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts"),
    )
}

async fn enqueue(
    store: &PostgresEventStore,
    ids: &SeededIds,
    clock: &FixedClock,
) -> koine_domain::JobId {
    let stream = ids.job_id();
    let event = Job::initial_event(
        QueueName::new("default").expect("q"),
        serde_json::json!({}),
        Priority(0),
        RetryPolicy::default(),
        None,
    );
    let envs = wrap_events(
        ids,
        clock,
        stream,
        0,
        ids.correlation_id(),
        None,
        None,
        vec![event],
    );
    store.append(stream, 0, envs).await.expect("append");
    stream
}

#[tokio::test]
async fn relays_in_order_and_deletes_on_success() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool.clone());
    let relay = PostgresOutboxRelay::new(pool.clone());
    let ids = SeededIds::new(41);
    let clock = clock();
    let a = enqueue(&store, &ids, &clock).await;
    let b = enqueue(&store, &ids, &clock).await;

    let sink = Collecting(Mutex::new(Vec::new()));
    assert_eq!(relay.relay_once(&sink, 10).await.expect("relay"), 2);
    let seen = sink.0.lock().expect("lock").clone();
    assert_eq!(
        seen,
        vec![format!("{a}:enqueued"), format!("{b}:enqueued")],
        "outbox order"
    );

    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.outbox")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(left, 0, "delivered rows deleted");
    assert_eq!(
        relay.relay_once(&sink, 10).await.expect("relay"),
        0,
        "drained"
    );
}

#[tokio::test]
async fn sink_failure_rolls_back_for_redelivery() {
    let (_guard, pool) = support::pg().await;
    let store = PostgresEventStore::new(pool.clone());
    let relay = PostgresOutboxRelay::new(pool.clone());
    let ids = SeededIds::new(42);
    let clock = clock();
    enqueue(&store, &ids, &clock).await;

    let err = relay.relay_once(&Failing, 10).await.expect_err("sink down");
    assert!(matches!(err, koine_application::RelayError::Sink(_)));
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.outbox")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(left, 1, "failed batch stays for redelivery");

    let sink = Collecting(Mutex::new(Vec::new()));
    assert_eq!(
        relay.relay_once(&sink, 10).await.expect("relay"),
        1,
        "redelivered"
    );
}
