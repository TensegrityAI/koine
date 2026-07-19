//! In-memory dispatch signal and no-op presence adapters for tests.
//!
//! Note: This module uses the RPITIT pattern for trait implementations
//! (`impl Future<Output = ()> + Send`), which necessarily produces `manual_async_fn`.

#![allow(clippy::manual_async_fn)]

use koine_application::ports::{DispatchSignal, WorkerPresence};
use koine_domain::{QueueName, WorkerId};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// In-memory dispatch signal: notifies waiters when a queue has claimable work.
pub struct NotifySignal {
    channels: Arc<Mutex<HashMap<QueueName, Arc<tokio::sync::Notify>>>>,
}

impl NotifySignal {
    /// Creates a new signal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            channels: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl Default for NotifySignal {
    fn default() -> Self {
        Self::new()
    }
}

impl DispatchSignal for NotifySignal {
    fn notify(&self, queue: &QueueName) -> impl Future<Output = ()> + Send {
        let queue = queue.clone();
        let channels = Arc::clone(&self.channels);
        async move {
            let mut map = channels.lock().await;
            let notify = map
                .entry(queue)
                .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                .clone();
            drop(map);
            notify.notify_waiters();
        }
    }

    fn wait(&self, queue: &QueueName, timeout: Duration) -> impl Future<Output = ()> + Send {
        let queue = queue.clone();
        let channels = Arc::clone(&self.channels);
        async move {
            let notify = {
                let mut map = channels.lock().await;
                map.entry(queue)
                    .or_insert_with(|| Arc::new(tokio::sync::Notify::new()))
                    .clone()
            };
            let _ = tokio::time::timeout(timeout, notify.notified()).await;
        }
    }
}

/// No-op worker presence: discards all updates (for test support).
pub struct NoopPresence;

impl WorkerPresence for NoopPresence {
    fn seen(
        &self,
        _worker: &WorkerId,
        _queue: Option<&QueueName>,
    ) -> impl Future<Output = ()> + Send {
        async move {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn wait_returns_promptly_after_concurrent_notify_same_queue() {
        let signal = NotifySignal::new();
        let queue = QueueName::new("default").expect("queue");
        let queue_clone = queue.clone();
        let signal_clone = Arc::new(signal);
        let signal_wait = Arc::clone(&signal_clone);

        let notify_task = {
            let q = queue_clone.clone();
            let s = Arc::clone(&signal_clone);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                s.notify(&q).await;
            })
        };

        let start = Instant::now();
        signal_wait.wait(&queue_clone, Duration::from_secs(5)).await;
        let elapsed = start.elapsed();

        notify_task.await.expect("task completed");
        assert!(
            elapsed < Duration::from_secs(1),
            "wait returned promptly after notify, elapsed: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn wait_on_different_queue_times_out_at_timeout() {
        let signal = NotifySignal::new();
        let queue1 = QueueName::new("default").expect("queue1");

        let start = Instant::now();
        signal.wait(&queue1, Duration::from_millis(100)).await;
        let elapsed = start.elapsed();

        assert!(
            elapsed >= Duration::from_millis(100),
            "wait timed out at ~100ms, elapsed: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_millis(200),
            "wait did not wait too long, elapsed: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn noop_presence_seen_completes() {
        let presence = NoopPresence;
        let worker = WorkerId::new("w1").expect("worker");
        let queue = QueueName::new("default").expect("queue");

        // Should complete without error
        presence.seen(&worker, Some(&queue)).await;
        presence.seen(&worker, None).await;
    }
}
