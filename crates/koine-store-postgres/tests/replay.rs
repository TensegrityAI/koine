//! Ring-3 replay test: `rebuild_dispatch` proves the dispatch projection is
//! derived state (ADR 0006) — folding every stream from zero must land on
//! the exact same `dispatch_queue` contents as the live, incremental
//! projection produced.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use koine_application::ports::{Clock as _, Dispatcher as _, EventStore as _, IdGenerator as _};
use koine_application::use_cases::worker_ack::WorkerAck;
use koine_application::wrap_events;
use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::{PostgresDispatcher, PostgresEventStore};
use sqlx::PgPool;

struct Fx {
    _guard: testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
    pool: PgPool,
    store: PostgresEventStore,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    dispatcher: PostgresDispatcher<SeededIds, FixedClock>,
    queue: QueueName,
    worker: WorkerId,
}

async fn fx() -> Fx {
    use chrono::TimeZone as _;
    let (guard, pool) = support::pg().await;
    let ids = Arc::new(SeededIds::new(41));
    let clock = Arc::new(FixedClock::at(
        chrono::Utc
            .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts"),
    ));
    Fx {
        _guard: guard,
        pool: pool.clone(),
        store: PostgresEventStore::new(pool.clone()),
        dispatcher: PostgresDispatcher::new(pool, Arc::clone(&ids), Arc::clone(&clock)),
        ids,
        clock,
        queue: QueueName::new("default").expect("q"),
        worker: WorkerId::new("w1").expect("w"),
    }
}

async fn enqueue(f: &Fx, priority: i16, not_before_secs: Option<u64>) -> JobId {
    let stream = f.ids.job_id();
    let now = f.clock.now();
    let not_before =
        not_before_secs.map(|s| now + chrono::TimeDelta::seconds(i64::try_from(s).expect("secs")));
    let event = Job::initial_event(
        f.queue.clone(),
        serde_json::json!({"job": stream.to_string()}),
        Priority(priority),
        RetryPolicy::default(),
        not_before,
    );
    let envs = wrap_events(
        f.ids.as_ref(),
        f.clock.as_ref(),
        stream,
        0,
        f.ids.correlation_id(),
        None,
        None,
        vec![event],
    );
    f.store.append(stream, 0, envs).await.expect("enqueue");
    stream
}

fn ack(f: &Fx) -> WorkerAck<'_, PostgresEventStore, SeededIds, FixedClock> {
    WorkerAck {
        store: &f.store,
        ids: f.ids.as_ref(),
        clock: f.clock.as_ref(),
    }
}

#[tokio::test]
async fn dispatch_queue_rebuilds_identically_from_the_log() {
    let f = fx().await;
    let ttl = Duration::from_secs(30);

    // build a mixed world: one pending, one scheduled, one leased, one done
    // (reuse Task 5's fixture helpers inline: enqueue x4, claim one, complete one)
    let highest = enqueue(&f, 9, None).await; // claimed and left leased
    let second = enqueue(&f, 7, None).await; // claimed, started, succeeded -> absent
    let _plain_pending = enqueue(&f, 3, None).await; // never claimed: stays pending
    let _scheduled = enqueue(&f, 1, Some(60)).await; // not_before in the future: stays scheduled

    // one lease_next claim, left leased (no start/ack)
    let leased = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, ttl)
        .await
        .expect("claim")
        .expect("job");
    assert_eq!(leased.job_id, highest, "priority order sanity");

    // one full succeed: lease -> start -> succeed via WorkerAck
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, ttl)
        .await
        .expect("claim")
        .expect("job");
    assert_eq!(claimed.job_id, second, "priority order sanity");
    ack(&f).start(second, &f.worker).await.expect("start");
    ack(&f)
        .succeed(
            second,
            &f.worker,
            claimed.lease,
            Some(serde_json::json!("done")),
        )
        .await
        .expect("succeed");

    // snapshot rows (ordered by queue, priority DESC, seq):
    let before: Vec<(uuid::Uuid, String, i16, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT job_id, queue, priority, lease_id FROM event_store.dispatch_queue \
         ORDER BY queue, priority DESC, seq",
    )
    .fetch_all(&f.pool)
    .await
    .expect("snapshot");
    assert_eq!(
        before.len(),
        3,
        "pending + scheduled + leased, succeeded is absent"
    );

    sqlx::query("TRUNCATE event_store.dispatch_queue")
        .execute(&f.pool)
        .await
        .expect("truncate");

    let rebuilt = koine_store_postgres::rebuild_dispatch(&f.pool)
        .await
        .expect("rebuild");
    assert_eq!(rebuilt as usize, before.len());

    let after: Vec<(uuid::Uuid, String, i16, Option<uuid::Uuid>)> = sqlx::query_as(
        "SELECT job_id, queue, priority, lease_id FROM event_store.dispatch_queue \
         ORDER BY queue, priority DESC, seq",
    )
    .fetch_all(&f.pool)
    .await
    .expect("resnapshot");
    // seq values are re-minted; ORDER and every other column must match
    assert_eq!(
        after, before,
        "projection replays from zero to identical state"
    );
}
