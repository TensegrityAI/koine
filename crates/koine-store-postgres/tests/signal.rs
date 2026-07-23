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
use std::future::{Future as _, poll_fn};
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

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
    let signal = PgSignal::connect(&url, pool.clone(), Duration::from_secs(5))
        .await
        .expect("connect listener");
    Fx {
        _guard: guard,
        pool: pool.clone(),
        store: PostgresEventStore::new(pool.clone()),
        signal,
        presence: PgPresence::new(pool),
        ids,
        clock,
        queue: QueueName::new("default").expect("q"),
        worker: WorkerId::new("w1").expect("w"),
    }
}

async fn append_job(
    store: &PostgresEventStore,
    ids: &SeededIds,
    clock: &FixedClock,
    queue: QueueName,
) {
    let stream = ids.job_id();
    let event = Job::initial_event(
        queue,
        serde_json::json!({"job": stream.to_string()}),
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
    store.append(stream, 0, envs).await.expect("enqueue");
}

async fn wait_after_subscribing(
    signal: Arc<PgSignal>,
    queue: QueueName,
    timeout: Duration,
    ready: oneshot::Sender<()>,
) {
    let mut wait = std::pin::pin!(signal.wait(&queue, timeout));
    let mut ready = Some(ready);
    poll_fn(|cx| match wait.as_mut().poll(cx) {
        Poll::Ready(()) => panic!("signal wait completed before it could subscribe"),
        Poll::Pending => {
            ready
                .take()
                .expect("readiness is signalled once")
                .send(())
                .expect("readiness receiver remains alive");
            Poll::Ready(())
        }
    })
    .await;
    wait.await;
}

async fn listener_pid(pool: &sqlx::PgPool, previous_pid: Option<i32>) -> i32 {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let pid = sqlx::query_scalar::<_, i32>(
                r"SELECT pid
                   FROM pg_stat_activity
                   WHERE datname = current_database()
                     AND query LIKE 'LISTEN%koine_dispatch%'
                     AND ($1::int IS NULL OR pid <> $1)
                   ORDER BY backend_start DESC
                   LIMIT 1",
            )
            .bind(previous_pid)
            .fetch_optional(pool)
            .await
            .expect("inspect listener activity");
            if let Some(pid) = pid {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("listener becomes visible in pg_stat_activity")
}

async fn listener_subscription_exists(pool: &sqlx::PgPool, pid: i32) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid = $1 AND query LIKE 'LISTEN%koine_dispatch%')",
    )
    .bind(pid)
    .fetch_one(pool)
    .await
    .expect("inspect listener backend")
}

async fn postgres_backend_exists(pool: &sqlx::PgPool, pid: i32) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid = $1)")
        .bind(pid)
        .fetch_one(pool)
        .await
        .expect("inspect postgres backend")
}

async fn wait_for_listener_backend_to_disappear(pool: &sqlx::PgPool, pid: i32) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while postgres_backend_exists(pool, pid).await {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("listener backend disappears after its signal owner is dropped");
}

#[tokio::test]
async fn presence_skips_when_pool_is_saturated() {
    let f = fx_with_pool_size(NonZeroU32::new(1).expect("non-zero pool size")).await;
    let held = f.pool.acquire().await.expect("hold only connection");

    assert_eq!(f.presence.dropped_writes(), 0);
    tokio::time::timeout(
        Duration::from_millis(250),
        f.presence.seen(&f.worker, Some(&f.queue)),
    )
    .await
    .expect("best-effort presence must not wait for pool timeout");

    // The skip is otherwise silent (ADR 0015); the drop counter makes it
    // observable rather than only visible as a later stale `last_seen`.
    assert_eq!(
        f.presence.dropped_writes(),
        1,
        "a saturated-pool skip must be counted as a dropped write"
    );

    drop(held);
}

#[tokio::test]
async fn signal_wait_wakes_on_append_to_queue() {
    let f = fx().await;
    let queue = f.queue.clone();
    let queue_clone = queue.clone();
    let signal = Arc::new(f.signal);
    let signal_wait = Arc::clone(&signal);
    let queue_append = f.queue.clone();
    let (ready_tx, ready_rx) = oneshot::channel();

    let wait_task = tokio::spawn(async move {
        let start = std::time::Instant::now();
        wait_after_subscribing(signal_wait, queue_clone, Duration::from_secs(5), ready_tx).await;
        start.elapsed()
    });

    ready_rx.await.expect("wait subscribed");
    append_job(&f.store, &f.ids, &f.clock, queue_append).await;

    let elapsed = wait_task.await.expect("wait task completed");

    assert!(
        elapsed < Duration::from_secs(1),
        "signal wait returned promptly on append, elapsed: {elapsed:?}"
    );
}

#[tokio::test]
async fn thirty_two_waiters_share_one_listener_without_starving_append() {
    const WAITERS: usize = 32;

    let f = fx_with_pool_size(NonZeroU32::new(1).expect("non-zero pool size")).await;
    let signal = Arc::new(f.signal);
    let mut waits = Vec::with_capacity(WAITERS);
    let mut readiness = Vec::with_capacity(WAITERS);

    for _ in 0..WAITERS {
        let (ready_tx, ready_rx) = oneshot::channel();
        readiness.push(ready_rx);
        waits.push(tokio::spawn(wait_after_subscribing(
            Arc::clone(&signal),
            f.queue.clone(),
            Duration::from_secs(5),
            ready_tx,
        )));
    }
    for ready in readiness {
        ready.await.expect("wait subscribed");
    }
    let listener_count = sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND query LIKE 'LISTEN%koine_dispatch%'",
    )
    .fetch_one(&f.pool)
    .await
    .expect("count listener activity");
    assert_eq!(listener_count, 1, "all waits share one listener backend");

    tokio::time::timeout(
        Duration::from_secs(1),
        append_job(&f.store, &f.ids, &f.clock, f.queue.clone()),
    )
    .await
    .expect("idle waits leave the size-one operational pool available");

    tokio::time::timeout(Duration::from_secs(1), async {
        for wait in waits {
            wait.await.expect("wait task completed");
        }
    })
    .await
    .expect("all waiters receive the one queue notification");
}

#[tokio::test]
async fn listener_reconnects_after_backend_termination() {
    let f = fx().await;
    let original_pid = listener_pid(&f.pool, None).await;
    let started = Instant::now();
    let terminated = sqlx::query_scalar::<_, bool>("SELECT pg_terminate_backend($1)")
        .bind(original_pid)
        .fetch_one(&f.pool)
        .await
        .expect("terminate listener backend");
    assert!(terminated, "listener backend was terminated");

    let signal = Arc::new(f.signal);
    let wait_signal = Arc::clone(&signal);
    let (ready_tx, ready_rx) = oneshot::channel();
    let wait = tokio::spawn(wait_after_subscribing(
        wait_signal,
        f.queue.clone(),
        Duration::from_secs(5),
        ready_tx,
    ));
    ready_rx.await.expect("wait subscribed");

    append_job(&f.store, &f.ids, &f.clock, f.queue.clone()).await;
    let reconnected_pid = listener_pid(&f.pool, Some(original_pid)).await;
    assert_ne!(reconnected_pid, original_pid);

    // Notifications are transient during reconnect. A second append after
    // LISTEN is visible proves prompt wakeup without making it correctness.
    append_job(&f.store, &f.ids, &f.clock, f.queue.clone()).await;
    let remaining = Duration::from_secs(2).saturating_sub(started.elapsed());
    tokio::time::timeout(remaining, wait)
        .await
        .expect("wait wakes within the reconnect budget")
        .expect("wait task completed");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn listener_lives_until_last_signal_clone_is_dropped() {
    let Fx {
        _guard,
        pool,
        signal,
        ..
    } = fx().await;
    let listener_pid = listener_pid(&pool, None).await;
    let remaining_signal = signal.clone();

    drop(signal);
    assert!(
        listener_subscription_exists(&pool, listener_pid).await,
        "dropping an intermediate clone keeps the listener alive"
    );
    drop(remaining_signal);

    wait_for_listener_backend_to_disappear(&pool, listener_pid).await;
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
