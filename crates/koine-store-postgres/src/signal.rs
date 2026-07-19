//! Postgres dispatch signal using NOTIFY/LISTEN.
//!
//! Note: This module uses the RPITIT pattern for trait implementations
//! (`impl Future<Output = ()> + Send`), which necessarily produces `manual_async_fn`.

#![allow(clippy::manual_async_fn)]

use koine_application::ports::DispatchSignal;
use koine_domain::QueueName;
use sqlx::PgPool;
use std::future::Future;
use std::time::Duration;

/// Postgres-backed dispatch signal using NOTIFY/LISTEN.
pub struct PgSignal {
    pool: PgPool,
}

impl PgSignal {
    /// Creates a new signal over the given pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl DispatchSignal for PgSignal {
    fn notify(&self, queue: &QueueName) -> impl Future<Output = ()> + Send {
        let pool = self.pool.clone();
        let queue_str = queue.as_str().to_string();
        async move {
            let _ = sqlx::query("SELECT pg_notify('koine_dispatch', $1)")
                .bind(queue_str)
                .execute(&pool)
                .await;
        }
    }

    fn wait(&self, queue: &QueueName, timeout: Duration) -> impl Future<Output = ()> + Send {
        let pool = self.pool.clone();
        let queue_str = queue.as_str().to_string();
        async move {
            if let Ok(mut listener) = sqlx::postgres::PgListener::connect_with(&pool).await {
                let _ = listener.listen("koine_dispatch").await;
                let mut remaining = timeout;
                loop {
                    let start = std::time::Instant::now();
                    match tokio::time::timeout(remaining, listener.recv()).await {
                        Ok(Ok(notification)) => {
                            if notification.payload() == queue_str {
                                return;
                            }
                            remaining = remaining.saturating_sub(start.elapsed());
                            if remaining.is_zero() {
                                return;
                            }
                        }
                        Ok(Err(_)) | Err(_) => return,
                    }
                }
            }
        }
    }
}
