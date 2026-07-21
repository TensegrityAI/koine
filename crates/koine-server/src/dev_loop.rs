//! `dev-loop`: exercises the entire 1B stack end-to-end against a real
//! database — enqueue, worker loop, sweep, outbox relay — and prints each
//! job's recorded story (`DoD` "exercised as a product").

use std::sync::Arc;
use std::time::Duration;

use koine_application::Lineage;
use koine_application::ports::EventStore as _;
use koine_application::use_cases::enqueue::{EnqueueCommand, EnqueueJob};
use koine_application::use_cases::lease::LeaseNextJob;
use koine_application::use_cases::sweep::SweepExpiredLeases;
use koine_application::use_cases::worker_ack::WorkerAck;
use koine_domain::{JobError, JobId, Priority, QueueName, RetryPolicy, WorkerId};
use koine_store_postgres::{
    PoolConfig, PostgresDispatcher, PostgresEventStore, PostgresOutboxRelay, connect_pool,
};
use serde_json::Value;

use crate::runtime::{SystemClock, UuidV7Ids};
use crate::sinks::PrintingSink;

/// The retry policy for the exercise: short enough that the flaky/crashy
/// paths resolve well inside the 60s budget.
fn dev_retry_policy() -> RetryPolicy {
    RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(500),
        max_delay: Duration::from_secs(2),
    }
}

/// Reads a boolean flag off a job payload (missing/non-bool = false).
fn flag(payload: &Value, name: &str) -> bool {
    payload.get(name).and_then(Value::as_bool).unwrap_or(false)
}

/// Loads a job's recorded event kinds, oldest first.
async fn story(store: &PostgresEventStore, job: JobId) -> Result<Vec<&'static str>, String> {
    let envelopes = store
        .load(job)
        .await
        .map_err(|e| format!("load {job}: {e}"))?;
    Ok(envelopes.iter().map(|env| env.event.kind()).collect())
}

/// The worker: leases from `queue` forever, deciding via payload flags plus
/// `LeasedJob::attempt`. A "crashy" job's first lease is dropped without any
/// ack (simulated crash) so the sweep must recover it; a "flaky" job fails
/// retryably on its first attempt and succeeds on the next; everything else
/// succeeds immediately. Runs until the caller aborts the task.
async fn worker_loop(
    store: Arc<PostgresEventStore>,
    dispatcher: Arc<PostgresDispatcher<UuidV7Ids, SystemClock>>,
    ids: Arc<UuidV7Ids>,
    clock: Arc<SystemClock>,
    queue: QueueName,
    ttl: Duration,
) -> Result<(), String> {
    let worker_id = WorkerId::new("dev-worker").map_err(|e| format!("worker id: {e}"))?;
    let lease_uc = LeaseNextJob {
        dispatcher: dispatcher.as_ref(),
    };
    let ack = WorkerAck {
        store: store.as_ref(),
        ids: ids.as_ref(),
        clock: clock.as_ref(),
    };
    loop {
        let Some(leased) = lease_uc
            .execute(&queue, &worker_id, ttl)
            .await
            .map_err(|e| format!("lease_next: {e}"))?
        else {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        };
        if flag(&leased.payload, "crashy") && leased.attempt == 0 {
            // Simulated crash: hold the lease and drop it on the floor — no
            // start, no ack. The sweep recovers it once the ttl elapses.
            continue;
        }
        ack.start(leased.job_id, &worker_id)
            .await
            .map_err(|e| format!("start {}: {e}", leased.job_id))?;
        if flag(&leased.payload, "flaky") && leased.attempt == 0 {
            let error = JobError {
                kind: "io".into(),
                message: "simulated transient io error".into(),
                stacktrace: None,
                retryable: true,
            };
            ack.fail(leased.job_id, &worker_id, leased.lease, error)
                .await
                .map_err(|e| format!("fail {}: {e}", leased.job_id))?;
        } else {
            ack.succeed(
                leased.job_id,
                &worker_id,
                leased.lease,
                Some(serde_json::json!("done")),
            )
            .await
            .map_err(|e| format!("succeed {}: {e}", leased.job_id))?;
        }
    }
}

/// Enqueues the exercise's three jobs on `queue`: job1 plain, job2 "crashy",
/// job3 "flaky" — and returns their ids in that order.
async fn enqueue_dev_jobs(
    store: &PostgresEventStore,
    ids: &UuidV7Ids,
    clock: &SystemClock,
    queue: &QueueName,
) -> Result<(JobId, JobId, JobId), String> {
    let enqueue = EnqueueJob { store, ids, clock };
    let mut enqueued = Vec::with_capacity(3);
    for (role, payload) in [
        ("job1 (plain)", serde_json::json!({})),
        ("job2 (crashy)", serde_json::json!({"crashy": true})),
        ("job3 (flaky)", serde_json::json!({"flaky": true})),
    ] {
        let job = enqueue
            .execute(EnqueueCommand {
                queue: queue.clone(),
                payload,
                priority: Priority(0),
                retry_policy: dev_retry_policy(),
                not_before: None,
                lineage: Lineage::default(),
            })
            .await
            .map_err(|e| format!("enqueue {role}: {e}"))?;
        enqueued.push(job);
    }
    Ok((enqueued[0], enqueued[1], enqueued[2]))
}

/// Loads each job's story, prints it, and checks for the markers that prove
/// the exercise: job1's story is exactly the plain happy path; job2's
/// contains `lease_expired` (crash recovery); job3's contains `failed` and
/// `retry_scheduled` (the flaky retry). Lists every missing marker in the
/// returned error rather than stopping at the first.
async fn check_stories(
    store: &PostgresEventStore,
    job1: JobId,
    job2: JobId,
    job3: JobId,
) -> Result<(), String> {
    let job1_story = story(store, job1).await?;
    let job2_story = story(store, job2).await?;
    let job3_story = story(store, job3).await?;
    println!("dev-loop: job1 (plain) story:  {}", job1_story.join(","));
    println!("dev-loop: job2 (crashy) story: {}", job2_story.join(","));
    println!("dev-loop: job3 (flaky) story:  {}", job3_story.join(","));

    let mut missing = Vec::new();
    if job1_story != ["enqueued", "leased", "started", "succeeded"] {
        missing.push(format!(
            "job1 (plain): expected enqueued,leased,started,succeeded — got {}",
            job1_story.join(",")
        ));
    }
    if !job2_story.contains(&"lease_expired") {
        missing.push("job2 (crashy): missing lease_expired".to_string());
    }
    if !(job3_story.contains(&"failed") && job3_story.contains(&"retry_scheduled")) {
        missing.push("job3 (flaky): missing failed and/or retry_scheduled".to_string());
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("missing story markers: {}", missing.join("; ")))
    }
}

/// Runs the dev-loop against `database_url`: enqueues three jobs, drives a
/// worker plus periodic sweep/outbox-relay ticks to completion, and verifies
/// each job's recorded story contains the expected markers.
///
/// # Errors
///
/// Returns an error message if connecting/migrating fails, any use case
/// fails, the 60s timeout elapses before all jobs reach `succeeded`, or a
/// job's recorded story is missing an expected marker.
pub async fn run(database_url: &str) -> Result<(), String> {
    // 1. connect + migrate; build store/dispatcher/relay with SystemClock/UuidV7Ids
    let pool = connect_pool(database_url, PoolConfig::default())
        .await
        .map_err(|e| format!("connect/migrate: {e}"))?;
    let ids = Arc::new(UuidV7Ids);
    let clock = Arc::new(SystemClock);
    let store = Arc::new(PostgresEventStore::new(pool.clone()));
    let dispatcher = Arc::new(PostgresDispatcher::new(
        pool.clone(),
        Arc::clone(&ids),
        Arc::clone(&clock),
    ));
    let relay = PostgresOutboxRelay::new(pool);
    let queue = QueueName::new("dev").map_err(|e| format!("queue: {e}"))?;
    let lease_ttl = Duration::from_secs(2);

    // 2. enqueue 3 jobs on queue "dev": job1 plain, job2 "crashy", job3 "flaky"
    let (job1, job2, job3) =
        enqueue_dev_jobs(store.as_ref(), ids.as_ref(), clock.as_ref(), &queue).await?;
    println!("dev-loop: enqueued job1(plain)={job1} job2(crashy)={job2} job3(flaky)={job3}");

    // 3. spawn worker task: loop { lease_next("dev") -> decide via payload
    //    flags + attempt -> start/succeed/fail; sleep 100ms when idle }
    let worker = tokio::spawn(worker_loop(
        Arc::clone(&store),
        Arc::clone(&dispatcher),
        Arc::clone(&ids),
        Arc::clone(&clock),
        queue.clone(),
        lease_ttl,
    ));

    // 4 + 5. main loop every 300ms: sweep, relay, then poll job states until
    // all 3 are succeeded or the 60s budget runs out.
    let sweep = SweepExpiredLeases {
        dispatcher: dispatcher.as_ref(),
    };
    let sink = PrintingSink;
    let tracked = [job1, job2, job3];
    let mut ticker = tokio::time::interval(Duration::from_millis(300));
    let deadline = tokio::time::Instant::now() + Duration::from_mins(1);
    loop {
        ticker.tick().await;
        sweep.execute().await.map_err(|e| format!("sweep: {e}"))?;
        relay
            .relay_once(&sink, 64)
            .await
            .map_err(|e| format!("relay: {e}"))?;

        let mut all_succeeded = true;
        for job in tracked {
            if !story(&store, job).await?.contains(&"succeeded") {
                all_succeeded = false;
            }
        }
        if all_succeeded {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            worker.abort();
            return Err("dev-loop timed out after 60s waiting for jobs to terminate".into());
        }
    }
    worker.abort();

    // 6. print each job's full kind story; assert the expected markers,
    // returning Err listing any that are missing.
    check_stories(store.as_ref(), job1, job2, job3).await
}
