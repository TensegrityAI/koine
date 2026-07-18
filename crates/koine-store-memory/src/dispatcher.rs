//! In-memory `Dispatcher`: the claim-and-lease composite of ADR 0011-b,
//! atomic under the store's single mutex exactly as the Postgres adapter
//! will be atomic under one transaction.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use koine_application::ports::{
    Clock, DispatchError, Dispatcher, EventStoreError, IdGenerator, LeasedJob,
};
use koine_application::wrap_events;
use koine_domain::{Job, JobId, LeaseId, QueueName, WorkerId};

use crate::store::{InMemoryEventStore, Inner};

/// Dispatcher over the in-memory store.
pub struct InMemoryDispatcher<G, C> {
    store: Arc<InMemoryEventStore>,
    ids: Arc<G>,
    clock: Arc<C>,
}

impl<G: IdGenerator, C: Clock> InMemoryDispatcher<G, C> {
    /// New dispatcher sharing the store's state.
    #[must_use]
    pub fn new(store: Arc<InMemoryEventStore>, ids: Arc<G>, clock: Arc<C>) -> Self {
        Self { store, ids, clock }
    }

    fn pick_eligible(inner: &Inner, queue: &QueueName, now: DateTime<Utc>) -> Option<JobId> {
        inner
            .index
            .iter()
            .filter(|(_, entry)| {
                entry.queue == *queue
                    && entry.lease.is_none()
                    && entry.not_before.is_none_or(|t| t <= now)
            })
            .max_by_key(|(_, entry)| (entry.priority, std::cmp::Reverse(entry.seq)))
            .map(|(job_id, _)| *job_id)
    }

    fn claim(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeasedJob>, DispatchError> {
        let now = self.clock.now();
        self.store
            .locked(|inner| {
                let Some(job_id) = Self::pick_eligible(inner, queue, now) else {
                    return Ok(None);
                };
                let stream = inner
                    .streams
                    .get(&job_id)
                    .cloned()
                    .ok_or(EventStoreError::StreamNotFound(job_id))?;
                let job = Job::from_events(&stream)
                    .map_err(|e| EventStoreError::Backend(format!("fold: {e}")))?;
                let lease = self.ids.lease_id();
                let event = job
                    .lease(worker.clone(), lease, now, ttl)
                    .map_err(|e| EventStoreError::Backend(format!("index/state drift: {e}")))?;
                let (correlation_id, causation_id, traceparent) =
                    koine_application::lineage_of(&stream);
                let envelopes = wrap_events(
                    self.ids.as_ref(),
                    self.clock.as_ref(),
                    job_id,
                    job.version,
                    correlation_id,
                    causation_id,
                    traceparent.clone(),
                    vec![event],
                );
                let expires_at = match &envelopes[0].event {
                    koine_domain::JobEvent::Leased { expires_at, .. } => *expires_at,
                    _ => return Err(EventStoreError::Backend("lease produced non-lease".into())),
                };
                InMemoryEventStore::append_locked(inner, job_id, job.version, envelopes)?;
                Ok(Some(LeasedJob {
                    job_id,
                    queue: job.queue,
                    payload: job.payload,
                    attempt: job.attempt,
                    lease,
                    expires_at,
                    correlation_id,
                    traceparent,
                }))
            })
            .map_err(DispatchError::from)
    }
}

impl<G: IdGenerator, C: Clock> Dispatcher for InMemoryDispatcher<G, C> {
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send {
        let result = self.claim(queue, worker, ttl);
        async move { result }
    }

    fn extend_lease(
        &self,
        lease: LeaseId,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool, DispatchError>> + Send {
        let now = self.clock.now();
        let deadline = now + chrono::TimeDelta::from_std(ttl).unwrap_or(chrono::TimeDelta::MAX);
        let result = self
            .store
            .locked(|inner| {
                for entry in inner.index.values_mut() {
                    if let Some(state) = entry.lease.as_mut()
                        && state.lease == lease
                    {
                        if state.expires_at <= now {
                            return Ok(false);
                        }
                        state.expires_at = deadline;
                        return Ok(true);
                    }
                }
                Ok(false)
            })
            .map_err(DispatchError::from);
        async move { result }
    }

    fn expired(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<JobId>, DispatchError>> + Send {
        let result = self
            .store
            .locked(|inner| {
                let mut ids: Vec<JobId> = inner
                    .index
                    .iter()
                    .filter(|(_, entry)| entry.lease.as_ref().is_some_and(|l| l.expires_at <= now))
                    .map(|(id, _)| *id)
                    .collect();
                ids.sort();
                Ok(ids)
            })
            .map_err(DispatchError::from);
        async move { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryEventStore;
    use crate::test_support::{FixedClock, SeededIds};
    use chrono::{TimeZone, Utc};
    use koine_application::ports::EventStore;
    use koine_application::wrap_events;
    use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy, WorkerId};
    use std::sync::Arc;
    use std::time::Duration;

    struct Fixture {
        store: Arc<InMemoryEventStore>,
        ids: Arc<SeededIds>,
        clock: Arc<FixedClock>,
        dispatcher: InMemoryDispatcher<SeededIds, FixedClock>,
        queue: QueueName,
        worker: WorkerId,
    }

    fn fixture() -> Fixture {
        let store = Arc::new(InMemoryEventStore::new());
        let ids = Arc::new(SeededIds::new(9));
        let clock = Arc::new(FixedClock::at(
            Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
                .single()
                .expect("ts"),
        ));
        let dispatcher =
            InMemoryDispatcher::new(Arc::clone(&store), Arc::clone(&ids), Arc::clone(&clock));
        Fixture {
            store,
            ids,
            clock,
            dispatcher,
            queue: QueueName::new("default").expect("q"),
            worker: WorkerId::new("w1").expect("w"),
        }
    }

    async fn enqueue(f: &Fixture, priority: i16, not_before_secs: Option<u64>) -> JobId {
        let stream = f.ids.job_id();
        let correlation = f.ids.correlation_id();
        let now = koine_application::ports::Clock::now(f.clock.as_ref());
        let not_before = not_before_secs
            .map(|s| now + chrono::TimeDelta::seconds(i64::try_from(s).expect("secs")));
        let event = Job::initial_event(
            f.queue.clone(),
            serde_json::json!({"job": stream.to_string()}),
            Priority(priority),
            RetryPolicy::default(),
            not_before,
        );
        let envelopes = wrap_events(
            f.ids.as_ref(),
            f.clock.as_ref(),
            stream,
            0,
            correlation,
            None,
            None,
            vec![event],
        );
        f.store.append(stream, 0, envelopes).await.expect("enqueue");
        stream
    }

    #[tokio::test]
    async fn claims_by_priority_then_fifo() {
        let f = fixture();
        let low_first = enqueue(&f, 0, None).await;
        let high = enqueue(&f, 9, None).await;
        let low_second = enqueue(&f, 0, None).await;

        let ttl = Duration::from_secs(30);
        let first = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, ttl)
            .await
            .expect("claim");
        let second = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, ttl)
            .await
            .expect("claim");
        let third = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, ttl)
            .await
            .expect("claim");
        let fourth = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, ttl)
            .await
            .expect("claim");

        assert_eq!(first.expect("job").job_id, high, "highest priority first");
        assert_eq!(second.expect("job").job_id, low_first, "then FIFO");
        assert_eq!(third.expect("job").job_id, low_second);
        assert!(fourth.is_none(), "queue drained");
    }

    #[tokio::test]
    async fn respects_not_before() {
        let f = fixture();
        enqueue(&f, 0, Some(60)).await;
        let ttl = Duration::from_secs(30);
        assert!(
            f.dispatcher
                .lease_next(&f.queue, &f.worker, ttl)
                .await
                .expect("claim")
                .is_none(),
            "scheduled job must not be claimable yet"
        );
        f.clock.advance(Duration::from_secs(61));
        assert!(
            f.dispatcher
                .lease_next(&f.queue, &f.worker, ttl)
                .await
                .expect("claim")
                .is_some()
        );
    }

    #[tokio::test]
    async fn claim_appends_the_leased_event() {
        let f = fixture();
        let job_id = enqueue(&f, 0, None).await;
        let claimed = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .expect("job");
        assert_eq!(claimed.job_id, job_id);
        let stream = f.store.load(job_id).await.expect("load");
        assert_eq!(stream.len(), 2);
        assert_eq!(stream[1].event.kind(), "leased");
        assert_eq!(
            stream[1].correlation_id, stream[0].correlation_id,
            "lineage carried"
        );
    }

    #[tokio::test]
    async fn extend_and_expiry() {
        let f = fixture();
        enqueue(&f, 0, None).await;
        let claimed = f
            .dispatcher
            .lease_next(&f.queue, &f.worker, Duration::from_secs(30))
            .await
            .expect("claim")
            .expect("job");

        let now = koine_application::ports::Clock::now(f.clock.as_ref());
        assert!(f.dispatcher.expired(now).await.expect("expired").is_empty());

        f.clock.advance(Duration::from_secs(20));
        assert!(
            f.dispatcher
                .extend_lease(claimed.lease, Duration::from_secs(30))
                .await
                .expect("hb"),
            "live lease extends"
        );

        f.clock.advance(Duration::from_secs(31));
        let now = koine_application::ports::Clock::now(f.clock.as_ref());
        assert_eq!(
            f.dispatcher.expired(now).await.expect("expired"),
            vec![claimed.job_id]
        );
        assert!(
            !f.dispatcher
                .extend_lease(claimed.lease, Duration::from_secs(30))
                .await
                .expect("hb"),
            "expired lease refuses extension"
        );
    }
}
