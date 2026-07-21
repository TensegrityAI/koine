//! Ring-3 dispatch signal and presence tests: reactive wakeup and worker tracking.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use koine_application::ports::{
    DispatchSignal as _, EventStore as _, IdGenerator as _, WorkerPresence as _,
};
use koine_application::wrap_events;
use koine_domain::{Job, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_memory::{FixedClock, SeededIds};
use koine_store_postgres::{PgPresence, PgSignal, PoolConfig, PostgresEventStore, connect_pool};
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

// Type alias for presence row with timestamps
type PresenceRow = (
    String,
    chrono::DateTime<chrono::Utc>,
    chrono::DateTime<chrono::Utc>,
    Option<String>,
);

struct Fx {
    _guard: testcontainers::ContainerAsync<testcontainers_modules::postgres::Postgres>,
    pool: sqlx::PgPool,
    store: PostgresEventStore,
    ids: Arc<SeededIds>,
    clock: Arc<FixedClock>,
    signal: PgSignal,
    presence: PgPresence,
    queue: QueueName,
    worker: WorkerId,
}

async fn fx() -> Fx {
    fx_with_pool_size(NonZeroU32::new(16).expect("non-zero pool size")).await
}

async fn fx_with_pool_size(max_connections: NonZeroU32) -> Fx {
    use chrono::TimeZone as _;
    let (guard, url) = support::postgres_url().await;
    let pool = connect_pool(
        &url,
        PoolConfig::new(
            max_connections,
            NonZeroU64::new(5_000).expect("non-zero acquire timeout"),
        ),
    )
    .await
    .expect("connect + migrate");
    tokio::time::timeout(Duration::from_secs(1), async {
        while pool.num_idle() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("pool exposes an idle connection to best-effort adapters");
    let ids = Arc::new(SeededIds::new(31));
    let clock = Arc::new(FixedClock::at(
        chrono::Utc
            .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts"),
    ));
    let pool_clone1 = pool.clone();
    let pool_clone2 = pool.clone();
    let pool_clone3 = pool.clone();
    Fx {
        _guard: guard,
        pool: pool_clone3,
        store: PostgresEventStore::new(pool.clone()),
        signal: PgSignal::new(pool_clone1),
        presence: PgPresence::new(pool_clone2),
        ids,
        clock,
        queue: QueueName::new("default").expect("q"),
        worker: WorkerId::new("w1").expect("w"),
    }
}

#[tokio::test]
async fn presence_skips_when_pool_is_saturated() {
    let f = fx_with_pool_size(NonZeroU32::new(1).expect("non-zero pool size")).await;
    let held = f.pool.acquire().await.expect("hold only connection");

    tokio::time::timeout(
        Duration::from_millis(250),
        f.presence.seen(&f.worker, Some(&f.queue)),
    )
    .await
    .expect("best-effort presence must not wait for pool timeout");

    drop(held);
}

#[tokio::test]
async fn signal_wait_wakes_on_append_to_queue() {
    let f = fx().await;
    let queue = f.queue.clone();
    let queue_clone = queue.clone();
    let signal = Arc::new(f.signal);
    let signal_wait = Arc::clone(&signal);
    let store = Arc::new(f.store);
    let store_append = Arc::clone(&store);
    let ids = Arc::clone(&f.ids);
    let clock = Arc::clone(&f.clock);
    let queue_append = f.queue.clone();

    let wait_task = tokio::spawn(async move {
        let start = std::time::Instant::now();
        signal_wait.wait(&queue_clone, Duration::from_secs(5)).await;
        start.elapsed()
    });

    // Give the wait to settle
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Append a job to the queue (which should trigger NOTIFY)
    let stream = ids.job_id();
    let event = Job::initial_event(
        queue_append,
        serde_json::json!({"job": stream.to_string()}),
        Priority(0),
        RetryPolicy::default(),
        None,
    );
    let envs = wrap_events(
        ids.as_ref(),
        clock.as_ref(),
        stream,
        0,
        ids.correlation_id(),
        None,
        None,
        vec![event],
    );
    store_append.append(stream, 0, envs).await.expect("enqueue");

    let elapsed = wait_task.await.expect("wait task completed");

    assert!(
        elapsed < Duration::from_secs(1),
        "signal wait returned promptly on append, elapsed: {elapsed:?}"
    );
}

#[tokio::test]
async fn signal_wait_on_other_queue_times_out() {
    let f = fx().await;
    let queue1 = f.queue.clone();

    let start = std::time::Instant::now();
    f.signal.wait(&queue1, Duration::from_millis(300)).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed >= Duration::from_millis(300),
        "wait timed out at ~300ms, elapsed: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_millis(500),
        "wait did not wait too long, elapsed: {elapsed:?}"
    );
}

#[tokio::test]
async fn presence_records_worker_with_queue() {
    let f = fx().await;
    let worker = f.worker.clone();
    let queue = f.queue.clone();

    // Record presence with a queue
    f.presence.seen(&worker, Some(&queue)).await;

    // Sleep to ensure time gap between seen calls
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Record presence again
    f.presence.seen(&worker, Some(&queue)).await;

    // Query the table directly to verify last_seen and first_seen advanced
    let rows: Vec<PresenceRow> = sqlx::query_as(
        "SELECT worker_id, first_seen, last_seen, last_queue FROM event_store.workers WHERE worker_id = $1",
    )
    .bind(worker.as_str())
    .fetch_all(&f.pool)
    .await
    .expect("query");

    assert_eq!(rows.len(), 1, "exactly one worker row");
    assert_eq!(rows[0].0, worker.as_str(), "worker_id matches");
    assert!(rows[0].2 >= rows[0].1, "last_seen >= first_seen");
    assert!(
        rows[0].2 > rows[0].1,
        "last_seen > first_seen (time gap between seen calls)"
    );
    assert_eq!(
        rows[0].3,
        Some(queue.as_str().to_string()),
        "last_queue set"
    );

    // Record presence without a queue; last_queue should be preserved via COALESCE
    f.presence.seen(&worker, None).await;

    let rows_after: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT worker_id, last_queue FROM event_store.workers WHERE worker_id = $1",
    )
    .bind(worker.as_str())
    .fetch_all(&f.pool)
    .await
    .expect("query");

    assert_eq!(rows_after.len(), 1, "still exactly one worker row");
    assert_eq!(
        rows_after[0].1,
        Some(queue.as_str().to_string()),
        "last_queue preserved after seen(worker, None)"
    );
}
