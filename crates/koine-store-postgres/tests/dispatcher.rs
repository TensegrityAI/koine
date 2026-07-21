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

    let (left, right) = tokio::join!(
        f.dispatcher.retire_next_expired_lease(),
        f.dispatcher.retire_next_expired_lease(),
    );
    let retired = [left.expect("left"), right.expect("right")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert_eq!(retired, vec![claimed.job_id]);
    let kinds = f
        .store
        .load(claimed.job_id)
        .await
        .expect("load")
        .into_iter()
        .map(|e| e.event.kind())
        .collect::<Vec<_>>();
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == "lease_expired")
            .count(),
        1
    );
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

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS (\
                 SELECT 1 FROM pg_stat_activity \
                 WHERE query LIKE 'UPDATE event_store.dispatch_queue SET lease_expires_at%' \
                   AND wait_event_type = 'Lock')",
            )
            .fetch_one(&f.pool)
            .await
            .expect("inspect blocked heartbeat");
            if waiting {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("heartbeat reaches locked row before old deadline");

    f.clock.advance(Duration::from_secs(11));
    assert_eq!(
        f.dispatcher
            .retire_next_expired_lease()
            .await
            .expect("retire while locked"),
        None
    );
    lock.commit().await.expect("release dispatch row");
    assert!(heartbeat.await.expect("heartbeat task").expect("heartbeat"));
    assert_eq!(
        f.dispatcher
            .retire_next_expired_lease()
            .await
            .expect("retire after heartbeat"),
        None
    );
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
