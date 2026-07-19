//! Postgres worker presence tracker.
//!
//! Note: This module uses the RPITIT pattern for trait implementations
//! (`impl Future<Output = ()> + Send`), which necessarily produces `manual_async_fn`.

#![allow(clippy::manual_async_fn)]

use koine_application::ports::WorkerPresence;
use koine_domain::{QueueName, WorkerId};
use sqlx::PgPool;
use std::future::Future;

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
    fn seen(
        &self,
        worker: &WorkerId,
        queue: Option<&QueueName>,
    ) -> impl Future<Output = ()> + Send {
        let pool = self.pool.clone();
        let worker_id = worker.as_str().to_string();
        let last_queue = queue.map(|q| q.as_str().to_string());
        async move {
            // Presence is best-effort; we swallow DB errors by design (ADR 0015).
            // This ensures presence tracking never fails requests.
            let _ = sqlx::query(
                "INSERT INTO event_store.workers (worker_id, first_seen, last_seen, last_queue) \
                 VALUES ($1, now(), now(), $2) \
                 ON CONFLICT (worker_id) DO UPDATE SET \
                 last_seen = now(), \
                 last_queue = COALESCE($2, event_store.workers.last_queue)",
            )
            .bind(worker_id)
            .bind(last_queue)
            .execute(&pool)
            .await;
        }
    }
}
