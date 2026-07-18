//! Operator/agent cancellation.

use koine_domain::{Job, JobId};

use crate::lineage::wrap_events;
use crate::ports::{Clock, EventStore, IdGenerator};
use crate::use_cases::worker_ack::AckError;

/// Use case: cancel a job in any non-terminal state.
pub struct CancelJob<'a, S, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, G: IdGenerator, C: Clock> CancelJob<'_, S, G, C> {
    /// Appends `cancelled` (with optional reason).
    ///
    /// # Errors
    ///
    /// Returns an error if the store load fails, the domain rejects the cancellation,
    /// or the store append operation fails.
    pub async fn execute(&self, job_id: JobId, reason: Option<String>) -> Result<(), AckError> {
        let stream = self.store.load(job_id).await?;
        let job = Job::from_events(&stream)?;
        let event = job.cancel(reason)?;
        let correlation = stream.first().map_or_else(
            || koine_domain::CorrelationId::new(uuid::Uuid::nil()),
            |env| env.correlation_id,
        );
        let causation = stream.last().map(|env| env.event_id);
        let traceparent = stream.first().and_then(|env| env.traceparent.clone());
        let envelopes = wrap_events(
            self.ids,
            self.clock,
            job.id,
            job.version,
            correlation,
            causation,
            traceparent,
            vec![event],
        );
        self.store.append(job.id, job.version, envelopes).await?;
        Ok(())
    }
}
