//! `PostgresDispatcher`: the ADR 0011-b claim and ADR 0016 lease-retirement
//! composites as SQL transactions — row-lock selection, domain-derived
//! events, append and synchronous projection update, then commit.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use koine_application::lineage_of;
use koine_application::ports::{
    Clock, DispatchError, Dispatcher, EventStoreError, IdGenerator, LeasedJob,
};
use koine_application::wrap_events;
use koine_domain::{Job, JobEvent, JobId, LeaseId, QueueName, WorkerId};
use sqlx::PgPool;
use uuid::Uuid;

use crate::rows::db;
use crate::store::{append_in_tx, load_in_tx};

/// Dispatcher over Postgres.
pub struct PostgresDispatcher<G, C> {
    pool: PgPool,
    ids: Arc<G>,
    clock: Arc<C>,
}

impl<G: IdGenerator, C: Clock> PostgresDispatcher<G, C> {
    /// New dispatcher over a migrated pool.
    #[must_use]
    pub fn new(pool: PgPool, ids: Arc<G>, clock: Arc<C>) -> Self {
        Self { pool, ids, clock }
    }

    async fn claim(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeasedJob>, DispatchError> {
        let now = self.clock.now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let picked: Option<(Uuid,)> = sqlx::query_as(
            "SELECT job_id FROM event_store.dispatch_queue \
             WHERE queue = $1 AND lease_id IS NULL \
               AND (not_before IS NULL OR not_before <= $2) \
             ORDER BY priority DESC, seq \
             LIMIT 1 FOR UPDATE SKIP LOCKED",
        )
        .bind(queue.as_str())
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some((job_uuid,)) = picked else {
            return Ok(None);
        };
        let job_id = JobId::new(job_uuid);
        let stream = load_in_tx(&mut tx, job_id).await?;
        let job = Job::from_events(&stream)
            .map_err(|e| EventStoreError::Backend(format!("fold: {e}")))?;
        let lease = self.ids.lease_id();
        let event = job
            .lease(worker.clone(), lease, now, ttl)
            .map_err(|e| EventStoreError::Backend(format!("index/state drift: {e}")))?;
        let (correlation_id, causation_id, traceparent) = lineage_of(&stream);
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
        let JobEvent::Leased { expires_at, .. } = envelopes[0].event else {
            return Err(EventStoreError::Backend("lease produced non-lease".into()).into());
        };
        let folded = append_in_tx(&mut tx, job_id, job.version, &envelopes).await?;
        tx.commit().await.map_err(db)?;
        Ok(Some(LeasedJob {
            job_id,
            queue: folded.queue,
            payload: folded.payload,
            attempt: folded.attempt,
            lease,
            expires_at,
            correlation_id,
            traceparent,
        }))
    }

    async fn retire_one(&self) -> Result<Option<JobId>, DispatchError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let now = self.clock.now();
        let picked: Option<(Uuid,)> = sqlx::query_as(
            "SELECT job_id FROM event_store.dispatch_queue \
             WHERE lease_id IS NOT NULL AND lease_expires_at <= $1 \
             ORDER BY lease_expires_at, job_id \
             LIMIT 1 FOR UPDATE SKIP LOCKED",
        )
        .bind(now)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let Some((job_uuid,)) = picked else {
            tx.commit().await.map_err(db)?;
            return Ok(None);
        };
        let job_id = JobId::new(job_uuid);
        let stream = load_in_tx(&mut tx, job_id).await?;
        let job = Job::from_events(&stream)?;
        let events = job.expire_lease(now, self.ids.jitter_seed())?;
        let (correlation_id, causation_id, traceparent) = lineage_of(&stream);
        let envelopes = wrap_events(
            self.ids.as_ref(),
            self.clock.as_ref(),
            job_id,
            job.version,
            correlation_id,
            causation_id,
            traceparent,
            events,
        );
        append_in_tx(&mut tx, job_id, job.version, &envelopes).await?;
        tx.commit().await.map_err(db)?;
        Ok(Some(job_id))
    }
}

impl<G: IdGenerator, C: Clock> Dispatcher for PostgresDispatcher<G, C> {
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send {
        self.claim(queue, worker, ttl)
    }

    async fn extend_lease(&self, lease: LeaseId, ttl: Duration) -> Result<bool, DispatchError> {
        let Ok(delta) = chrono::TimeDelta::from_std(ttl) else {
            return Err(DispatchError::Backend("ttl out of range".into()));
        };
        let mut tx = self.pool.begin().await.map_err(db)?;
        let now = self.clock.now();
        let deadline = now + delta;
        let updated = sqlx::query(
            "UPDATE event_store.dispatch_queue SET lease_expires_at = $1 \
             WHERE lease_id = $2 AND lease_expires_at > $3",
        )
        .bind(deadline)
        .bind(lease.as_uuid())
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(updated.rows_affected() > 0)
    }

    fn retire_next_expired_lease(
        &self,
    ) -> impl Future<Output = Result<Option<JobId>, DispatchError>> + Send {
        self.retire_one()
    }
}
