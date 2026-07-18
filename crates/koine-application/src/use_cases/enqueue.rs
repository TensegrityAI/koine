//! Accepting new jobs into the log.

use chrono::{DateTime, Utc};
use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy};
use serde_json::Value;

use crate::lineage::{Lineage, wrap_events};
use crate::ports::{Clock, EventStore, EventStoreError, IdGenerator};

/// Command input for [`EnqueueJob`].
#[derive(Debug, Clone)]
pub struct EnqueueCommand {
    /// Destination queue.
    pub queue: QueueName,
    /// Opaque worker payload.
    pub payload: Value,
    /// Dispatch priority.
    pub priority: Priority,
    /// Retry policy for this job.
    pub retry_policy: RetryPolicy,
    /// Earliest dispatch time.
    pub not_before: Option<DateTime<Utc>>,
    /// Caller lineage.
    pub lineage: Lineage,
}

/// Use case: accept a new job.
pub struct EnqueueJob<'a, S, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, G: IdGenerator, C: Clock> EnqueueJob<'_, S, G, C> {
    /// Opens a new stream with `enqueued` (version 1) and returns the job id.
    ///
    /// # Errors
    ///
    /// Returns an error if the event store append operation fails.
    pub async fn execute(&self, cmd: EnqueueCommand) -> Result<JobId, EventStoreError> {
        let job_id = self.ids.job_id();
        let correlation = cmd
            .lineage
            .correlation_id
            .unwrap_or_else(|| self.ids.correlation_id());
        let event = Job::initial_event(
            cmd.queue,
            cmd.payload,
            cmd.priority,
            cmd.retry_policy,
            cmd.not_before,
        );
        let envelopes = wrap_events(
            self.ids,
            self.clock,
            job_id,
            0,
            correlation,
            cmd.lineage.causation_id,
            cmd.lineage.traceparent,
            vec![event],
        );
        self.store.append(job_id, 0, envelopes).await?;
        Ok(job_id)
    }
}
