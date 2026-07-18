//! Worker-facing acks: start, succeed, fail. A stale ack (lease no longer
//! held) is never dropped — it becomes a `late_ack_conflict` record
//! (spec §3: information is never lost).

use koine_domain::{
    DomainError, EventEnvelope, Job, JobError, JobId, LeaseId, ReportedOutcome, WorkerId,
};
use serde_json::Value;
use thiserror::Error;

use crate::lineage::{lineage_of, wrap_events};
use crate::ports::{Clock, EventStore, EventStoreError, IdGenerator};

/// Errors from worker acks.
#[derive(Debug, Error)]
pub enum AckError {
    /// Store failure.
    #[error(transparent)]
    Store(#[from] EventStoreError),
    /// Domain rejection that is not a stale-lease situation.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// How an ack was recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckOutcome {
    /// Recorded as the worker intended.
    Recorded,
    /// The lease was no longer held — recorded as a conflict.
    Conflict,
}

/// Use case: worker acks.
pub struct WorkerAck<'a, S, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, G: IdGenerator, C: Clock> WorkerAck<'_, S, G, C> {
    /// Worker signals execution started.
    ///
    /// # Errors
    ///
    /// Returns an error if the store load fails or the domain rejects the transition.
    pub async fn start(&self, job_id: JobId, worker: &WorkerId) -> Result<(), AckError> {
        let (job, stream) = self.load(job_id).await?;
        let event = job.start(worker)?;
        self.append(&job, &stream, vec![event]).await?;
        Ok(())
    }

    /// Worker reports success.
    ///
    /// # Errors
    ///
    /// Returns a store error if append fails. Returns a domain error via `AckError::Domain`
    /// for lease-unrelated transitions; stale leases become `AckOutcome::Conflict`.
    pub async fn succeed(
        &self,
        job_id: JobId,
        worker: &WorkerId,
        lease: LeaseId,
        result: Option<Value>,
    ) -> Result<AckOutcome, AckError> {
        let (job, stream) = self.load(job_id).await?;
        if let Ok(event) = job.succeed(lease, result) {
            self.append(&job, &stream, vec![event]).await?;
            Ok(AckOutcome::Recorded)
        } else {
            self.record_conflict(&job, &stream, worker, lease, ReportedOutcome::Succeeded)
                .await?;
            Ok(AckOutcome::Conflict)
        }
    }

    /// Worker reports failure; the retry decision rides the same append.
    ///
    /// # Errors
    ///
    /// Returns a store error if append fails. Returns a domain error via `AckError::Domain`
    /// for lease-unrelated transitions; stale leases become `AckOutcome::Conflict`.
    pub async fn fail(
        &self,
        job_id: JobId,
        worker: &WorkerId,
        lease: LeaseId,
        error: JobError,
    ) -> Result<AckOutcome, AckError> {
        let (job, stream) = self.load(job_id).await?;
        if let Ok(events) = job.fail(lease, error, self.clock.now(), self.ids.jitter_seed()) {
            self.append(&job, &stream, events).await?;
            Ok(AckOutcome::Recorded)
        } else {
            self.record_conflict(&job, &stream, worker, lease, ReportedOutcome::Failed)
                .await?;
            Ok(AckOutcome::Conflict)
        }
    }

    async fn load(&self, job_id: JobId) -> Result<(Job, Vec<EventEnvelope>), AckError> {
        let stream = self.store.load(job_id).await?;
        let job = Job::from_events(&stream)?;
        Ok((job, stream))
    }

    async fn append(
        &self,
        job: &Job,
        stream: &[EventEnvelope],
        events: Vec<koine_domain::JobEvent>,
    ) -> Result<(), EventStoreError> {
        let (correlation, causation, traceparent) = lineage_of(stream);
        let envelopes = wrap_events(
            self.ids,
            self.clock,
            job.id,
            job.version,
            correlation,
            causation,
            traceparent,
            events,
        );
        self.store.append(job.id, job.version, envelopes).await
    }

    async fn record_conflict(
        &self,
        job: &Job,
        stream: &[EventEnvelope],
        worker: &WorkerId,
        lease: LeaseId,
        reported: ReportedOutcome,
    ) -> Result<(), EventStoreError> {
        let event = Job::late_ack(worker.clone(), lease, reported);
        self.append(job, stream, vec![event]).await
    }
}
