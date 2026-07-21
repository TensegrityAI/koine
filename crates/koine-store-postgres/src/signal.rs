//! Postgres dispatch signal using NOTIFY/LISTEN.

use koine_application::ports::DispatchSignal;
use koine_domain::QueueName;
use sqlx::PgPool;
use sqlx::postgres::{PgListener, PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::AbortHandle;

const NOTIFICATION_BUFFER: usize = 1_024;
const RECONNECT_BACKOFF: Duration = Duration::from_millis(100);

struct SignalHub {
    notifications: broadcast::Sender<String>,
    listener_task: AbortHandle,
}

impl Drop for SignalHub {
    fn drop(&mut self) {
        self.listener_task.abort();
    }
}

/// Postgres-backed dispatch signal using NOTIFY/LISTEN.
///
/// Clones share one listener hub. Dropping the final clone cancels its
/// background task and releases the dedicated listener connection.
#[derive(Clone)]
pub struct PgSignal {
    notify_pool: PgPool,
    hub: Arc<SignalHub>,
}

impl PgSignal {
    /// Connects one dedicated listener and starts its in-process fan-out hub.
    ///
    /// The listener uses a separate size-one pool so idle waits never consume
    /// the operational pool used by [`Self::notify`] and the store adapters.
    ///
    /// # Errors
    ///
    /// Returns the initial listener connection or `LISTEN` error. The
    /// subscription is established before this function returns.
    pub async fn connect(
        url: &str,
        notify_pool: PgPool,
        listener_acquire_timeout: Duration,
    ) -> Result<Self, sqlx::Error> {
        let listener_pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(listener_acquire_timeout)
            .connect(url)
            .await?;
        let mut listener = PgListener::connect_with(&listener_pool).await?;
        listener.listen("koine_dispatch").await?;

        let (notifications, _) = broadcast::channel(NOTIFICATION_BUFFER);
        let fanout = notifications.clone();
        let listener_task = tokio::spawn(async move {
            loop {
                match listener.recv().await {
                    Ok(notification) => {
                        let _ = fanout.send(notification.payload().to_string());
                    }
                    Err(_) => tokio::time::sleep(RECONNECT_BACKOFF).await,
                }
            }
        });

        Ok(Self {
            notify_pool,
            hub: Arc::new(SignalHub {
                notifications,
                listener_task: listener_task.abort_handle(),
            }),
        })
    }
}

impl DispatchSignal for PgSignal {
    async fn notify(&self, queue: &QueueName) {
        let queue_str = queue.as_str().to_string();
        let _ = sqlx::query("SELECT pg_notify('koine_dispatch', $1)")
            .bind(queue_str)
            .execute(&self.notify_pool)
            .await;
    }

    async fn wait(&self, queue: &QueueName, timeout: Duration) {
        let mut receiver = self.hub.notifications.subscribe();
        let queue_str = queue.as_str();
        let _ = tokio::time::timeout(timeout, async {
            loop {
                match receiver.recv().await {
                    Ok(payload) if payload == queue_str => return,
                    Ok(_) => {}
                    Err(
                        broadcast::error::RecvError::Lagged(_)
                        | broadcast::error::RecvError::Closed,
                    ) => return,
                }
            }
        })
        .await;
    }
}
