//! Postgres worker presence tracker.

use koine_application::ports::WorkerPresence;
use koine_domain::{QueueName, WorkerId};
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const PRESENCE_WRITE_BUDGET: Duration = Duration::from_millis(100);
const PRESENCE_UPSERT: &str = "INSERT INTO event_store.workers (worker_id, first_seen, last_seen, last_queue) \
     VALUES ($1, now(), now(), $2) \
     ON CONFLICT (worker_id) DO UPDATE SET \
     last_seen = now(), \
     last_queue = COALESCE($2, event_store.workers.last_queue)";

/// Postgres-backed worker presence tracker.
pub struct PgPresence {
    pool: PgPool,
    // Best-effort presence (ADR 0015) drops writes rather than block requests:
    // a saturated pool or a write that overruns the budget records nothing.
    // Those drops are otherwise invisible (only a stale `last_seen` shows
    // downstream), so we count them for the phase-3 metrics surface and tests.
    dropped_writes: AtomicU64,
}

impl PgPresence {
    /// Creates a new presence tracker over the given pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            dropped_writes: AtomicU64::new(0),
        }
    }

    /// Number of best-effort presence writes dropped so far (pool saturated or
    /// budget overrun). Monotonic; observability for the otherwise-silent
    /// best-effort path (ADR 0015).
    #[must_use]
    pub fn dropped_writes(&self) -> u64 {
        self.dropped_writes.load(Ordering::Relaxed)
    }
}

impl WorkerPresence for PgPresence {
    async fn seen(&self, worker: &WorkerId, queue: Option<&QueueName>) {
        let Some(mut connection) = self.pool.try_acquire() else {
            self.dropped_writes.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let worker_id = worker.as_str().to_string();
        let last_queue = queue.map(|q| q.as_str().to_string());
        // Presence is best-effort; we swallow DB errors by design (ADR 0015).
        // This ensures presence tracking never fails requests. A budget
        // overrun or DB error leaves nothing written, so it counts as a drop.
        let outcome = tokio::time::timeout(
            PRESENCE_WRITE_BUDGET,
            sqlx::query(PRESENCE_UPSERT)
                .bind(worker_id)
                .bind(last_queue)
                .execute(&mut *connection),
        )
        .await;
        if !matches!(outcome, Ok(Ok(_))) {
            self.dropped_writes.fetch_add(1, Ordering::Relaxed);
        }
    }
}
