//! Postgres worker presence tracker.

use koine_application::ports::WorkerPresence;
use koine_domain::{QueueName, WorkerId};
use sqlx::PgPool;
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
}

impl PgPresence {
    /// Creates a new presence tracker over the given pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl WorkerPresence for PgPresence {
    async fn seen(&self, worker: &WorkerId, queue: Option<&QueueName>) {
        let Some(mut connection) = self.pool.try_acquire() else {
            return;
        };
        let worker_id = worker.as_str().to_string();
        let last_queue = queue.map(|q| q.as_str().to_string());
        // Presence is best-effort; we swallow DB errors by design (ADR 0015).
        // This ensures presence tracking never fails requests.
        let _ = tokio::time::timeout(
            PRESENCE_WRITE_BUDGET,
            sqlx::query(PRESENCE_UPSERT)
                .bind(worker_id)
                .bind(last_queue)
                .execute(&mut *connection),
        )
        .await;
    }
}
