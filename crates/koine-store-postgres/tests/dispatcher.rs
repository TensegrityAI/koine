//! Ring-3 dispatcher tests: the ADR 0011 claim composite over real SQL.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::sync::Arc;
use std::time::Duration;

use koine_application::ports::{Clock as _, Dispatcher as _, EventStore as _, IdGenerator as _};
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
    let ids = Arc::new(SeededIds::new(31));
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

async fn wait_until_query_waits_on_lock(pool: &PgPool, query_prefix: &str, context: &str) {
    let pattern = format!("{query_prefix}%");
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (\
                 SELECT 1 FROM pg_stat_activity \
                 WHERE query LIKE $1 AND wait_event_type = 'Lock')",
            )
            .bind(&pattern)
            .fetch_one(pool)
            .await
            .expect("inspect blocked query");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{context}"));
}

async fn expiry_count(f: &Fx, job: JobId) -> usize {
    f.store
        .load(job)
        .await
        .expect("load")
        .iter()
        .filter(|event| event.event.kind() == "lease_expired")
        .count()
}

#[tokio::test]
async fn claims_by_priority_then_fifo_and_appends_leased() {
    let f = fx().await;
    let low_first = enqueue(&f, 0, None).await;
    let high = enqueue(&f, 9, None).await;
    let ttl = Duration::from_secs(30);

    let first = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, ttl)
        .await
        .expect("claim");
    assert_eq!(first.expect("job").job_id, high, "priority first");

    let second = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, ttl)
        .await
        .expect("claim");
    let second = second.expect("job");
    assert_eq!(second.job_id, low_first, "then FIFO");

    let stream = f.store.load(second.job_id).await.expect("load");
    assert_eq!(stream[1].event.kind(), "leased");
    assert_eq!(
        stream[1].correlation_id, stream[0].correlation_id,
        "lineage carried"
    );

    assert!(
        f.dispatcher
            .lease_next(&f.queue, &f.worker, ttl)
            .await
            .expect("claim")
            .is_none(),
        "drained"
    );
}

#[tokio::test]
async fn respects_not_before_and_lease_expiry() {
    let f = fx().await;
    enqueue(&f, 0, Some(60)).await;
    let ttl = Duration::from_secs(30);
    assert!(
        f.dispatcher
            .lease_next(&f.queue, &f.worker, ttl)
            .await
            .expect("claim")
            .is_none()
    );
    f.clock.advance(Duration::from_secs(61));
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, ttl)
        .await
        .expect("claim")
        .expect("eligible now");

    assert!(
        f.dispatcher
            .extend_lease(claimed.lease, Duration::from_mins(1))
            .await
            .expect("hb")
    );
    f.clock.advance(Duration::from_secs(31));
    assert!(
        f.dispatcher
            .retire_next_expired_lease()
            .await
            .expect("retire")
            .is_none(),
        "extended"
    );
    f.clock.advance(Duration::from_secs(31));
    assert_eq!(
        f.dispatcher
            .retire_next_expired_lease()
            .await
            .expect("retire"),
        Some(claimed.job_id)
    );
    assert!(
        !f.dispatcher
            .extend_lease(claimed.lease, ttl)
            .await
            .expect("hb"),
        "expired refuses"
    );
}

#[tokio::test]
async fn heartbeat_first_fences_retirement() {
    let f = fx().await;
    let job = enqueue(&f, 0, None).await;
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");

    f.clock.advance(Duration::from_secs(20));
    assert!(
        f.dispatcher
            .extend_lease(claimed.lease, Duration::from_secs(30))
            .await
            .expect("heartbeat")
    );
    f.clock.advance(Duration::from_secs(11));

    assert_eq!(
        f.dispatcher
            .retire_next_expired_lease()
            .await
            .expect("retire"),
        None
    );
    assert_eq!(f.store.load(job).await.expect("load").len(), 2);
}

#[tokio::test]
async fn retirement_first_rejects_heartbeat() {
    let f = fx().await;
    enqueue(&f, 0, None).await;
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");

    f.clock.advance(Duration::from_secs(31));
    assert_eq!(
        f.dispatcher
            .retire_next_expired_lease()
            .await
            .expect("retire"),
        Some(claimed.job_id)
    );
    assert!(
        !f.dispatcher
            .extend_lease(claimed.lease, Duration::from_secs(30))
            .await
            .expect("heartbeat")
    );
}

#[tokio::test]
async fn concurrent_retirement_records_one_expiry() {
    let f = fx().await;
    enqueue(&f, 0, None).await;
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    f.clock.advance(Duration::from_secs(31));

    let mut event_lock = f.pool.begin().await.expect("begin event-table lock");
    sqlx::query("LOCK TABLE event_store.events IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *event_lock)
        .await
        .expect("lock events table");

    let left_dispatcher =
        PostgresDispatcher::new(f.pool.clone(), Arc::clone(&f.ids), Arc::clone(&f.clock));
    let right_dispatcher =
        PostgresDispatcher::new(f.pool.clone(), Arc::clone(&f.ids), Arc::clone(&f.clock));
    let left = tokio::spawn(async move { left_dispatcher.retire_next_expired_lease().await });

    wait_until_query_waits_on_lock(
        &f.pool,
        "SELECT stream_id, version, event_id",
        "first retirement did not reach the controlled event-table lock",
    )
    .await;

    let right = tokio::time::timeout(
        Duration::from_secs(2),
        right_dispatcher.retire_next_expired_lease(),
    )
    .await
    .expect("second retirement must observe the in-flight row lock")
    .expect("right retirement");
    assert_eq!(right, None);

    event_lock.commit().await.expect("release events table");
    assert_eq!(
        left.await.expect("left task").expect("left retirement"),
        Some(claimed.job_id)
    );
    assert_eq!(expiry_count(&f, claimed.job_id).await, 1);
}

#[tokio::test]
async fn skip_locked_retires_second_expired_lease() {
    let f = fx().await;
    let first = enqueue(&f, 0, None).await;
    let second = enqueue(&f, 0, None).await;
    let first_claim = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("first claim")
        .expect("first job");
    let second_claim = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("second claim")
        .expect("second job");
    assert_eq!(first_claim.job_id, first);
    assert_eq!(second_claim.job_id, second);
    f.clock.advance(Duration::from_secs(31));

    let mut first_lock = f.pool.begin().await.expect("begin first-row lock");
    sqlx::query("SELECT job_id FROM event_store.dispatch_queue WHERE job_id = $1 FOR UPDATE")
        .bind(first.as_uuid())
        .fetch_one(&mut *first_lock)
        .await
        .expect("lock first expired row");

    let retirement =
        PostgresDispatcher::new(f.pool.clone(), Arc::clone(&f.ids), Arc::clone(&f.clock));
    let retired = tokio::time::timeout(
        Duration::from_secs(2),
        retirement.retire_next_expired_lease(),
    )
    .await
    .expect("SKIP LOCKED must progress past the first candidate")
    .expect("retire second candidate");
    assert_eq!(retired, Some(second));
    assert_eq!(expiry_count(&f, first).await, 0);
    assert_eq!(expiry_count(&f, second).await, 1);

    first_lock.commit().await.expect("release first row");
    assert_eq!(
        retirement
            .retire_next_expired_lease()
            .await
            .expect("retire first candidate"),
        Some(first)
    );
    assert_eq!(
        retirement
            .retire_next_expired_lease()
            .await
            .expect("queue drained"),
        None
    );
    assert_eq!(expiry_count(&f, first).await, 1);
    assert_eq!(expiry_count(&f, second).await, 1);
}

#[tokio::test]
async fn locked_expired_row_does_not_beat_earlier_heartbeat() {
    let f = fx().await;
    enqueue(&f, 0, None).await;
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    f.clock.advance(Duration::from_secs(20));

    let mut lock = f.pool.begin().await.expect("begin lock");
    sqlx::query("SELECT job_id FROM event_store.dispatch_queue WHERE job_id = $1 FOR UPDATE")
        .bind(claimed.job_id.as_uuid())
        .fetch_one(&mut *lock)
        .await
        .expect("lock dispatch row");

    let heartbeat_dispatcher =
        PostgresDispatcher::new(f.pool.clone(), Arc::clone(&f.ids), Arc::clone(&f.clock));
    let heartbeat = tokio::spawn(async move {
        heartbeat_dispatcher
            .extend_lease(claimed.lease, Duration::from_secs(30))
            .await
    });

    wait_until_query_waits_on_lock(
        &f.pool,
        "UPDATE event_store.dispatch_queue SET lease_expires_at",
        "heartbeat did not reach the locked row before the old deadline",
    )
    .await;

    f.clock.advance(Duration::from_secs(11));
    // Bound the retire calls: if the fence regressed and retirement blocked on
    // the held row instead of skipping it (SKIP LOCKED), this would hang the
    // whole binary — a timeout turns that into a fast, legible failure.
    let retired_while_locked = tokio::time::timeout(
        Duration::from_secs(5),
        f.dispatcher.retire_next_expired_lease(),
    )
    .await
    .expect("retire must not block on the locked row (SKIP LOCKED)")
    .expect("retire while locked");
    assert_eq!(retired_while_locked, None);
    lock.commit().await.expect("release dispatch row");
    assert!(heartbeat.await.expect("heartbeat task").expect("heartbeat"));
    let retired_after_heartbeat = tokio::time::timeout(
        Duration::from_secs(5),
        f.dispatcher.retire_next_expired_lease(),
    )
    .await
    .expect("retire after heartbeat must not hang")
    .expect("retire after heartbeat");
    assert_eq!(retired_after_heartbeat, None);
}

// Ring-3 regression test mirroring `koine-store-memory`'s
// `extend_lease_rejects_unrepresentable_ttl` (see
// `retry-policy-ttl-bounds-hardening` AC3 / `phase-2-carryover-hardening`
// AC1): the memory and Postgres dispatchers share the same
// `chrono::TimeDelta::from_std` guard, but until now only the memory twin
// ever exercised `Duration::MAX` — the Postgres path was code-parity, not
// test-parity. Assertions are unchanged from the memory twin.
#[tokio::test]
async fn extend_lease_rejects_unrepresentable_ttl() {
    let f = fx().await;
    enqueue(&f, 0, None).await;
    let claimed = f
        .dispatcher
        .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
        .await
        .expect("claim")
        .expect("job");
    let err = f
        .dispatcher
        .extend_lease(claimed.lease, Duration::MAX)
        .await
        .expect_err("must reject");
    assert!(matches!(err, koine_application::DispatchError::Backend(_)));
}

#[tokio::test]
async fn concurrent_claims_get_distinct_jobs() {
    let f = fx().await;
    let a = enqueue(&f, 0, None).await;
    let b = enqueue(&f, 0, None).await;
    let w2 = WorkerId::new("w2").expect("w");
    let ttl = Duration::from_secs(30);
    let (r1, r2) = tokio::join!(
        f.dispatcher.lease_next(&f.queue, &f.worker, ttl),
        f.dispatcher.lease_next(&f.queue, &w2, ttl),
    );
    let j1 = r1.expect("claim 1").expect("job 1").job_id;
    let j2 = r2.expect("claim 2").expect("job 2").job_id;
    assert_ne!(j1, j2, "SKIP LOCKED: no double-claim");
    let mut got = [j1, j2];
    got.sort();
    let mut want = [a, b];
    want.sort();
    assert_eq!(got, want);
}
