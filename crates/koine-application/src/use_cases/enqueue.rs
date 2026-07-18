//! Accepting new jobs into the log.

use chrono::{DateTime, Utc};
use koine_domain::{Job, JobId, Priority, QueueName, RetryPolicy};
use serde_json::Value;
use thiserror::Error;

use crate::lineage::{Lineage, wrap_events};
use crate::ports::{Clock, EventStore, EventStoreError, IdGenerator};

/// Errors from enqueueing.
#[derive(Debug, Error)]
pub enum EnqueueError {
    /// The retry policy fails the sanity bounds (broker protects itself
    /// from client-supplied pathology — hardening item AC1).
    #[error("invalid retry policy: {0}")]
    InvalidPolicy(&'static str),
    /// Store failure.
    #[error(transparent)]
    Store(#[from] EventStoreError),
}

/// Longest delay any policy may request (30 days).
#[allow(clippy::duration_suboptimal_units)]
const MAX_SANE_DELAY: std::time::Duration = std::time::Duration::from_secs(60 * 60 * 24 * 30);
/// Most attempts any policy may request.
const MAX_SANE_ATTEMPTS: u32 = 10_000;

fn validate_policy(policy: &RetryPolicy) -> Result<(), EnqueueError> {
    if policy.max_attempts == 0 {
        return Err(EnqueueError::InvalidPolicy("max_attempts must be >= 1"));
    }
    if policy.max_attempts > MAX_SANE_ATTEMPTS {
        return Err(EnqueueError::InvalidPolicy(
            "max_attempts above sane ceiling",
        ));
    }
    if policy.base_delay > policy.max_delay {
        return Err(EnqueueError::InvalidPolicy("base_delay exceeds max_delay"));
    }
    if policy.max_delay > MAX_SANE_DELAY {
        return Err(EnqueueError::InvalidPolicy(
            "max_delay above 30-day ceiling",
        ));
    }
    Ok(())
}

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
    /// Returns an error if the retry policy fails sanity bounds, or if the event store append operation fails.
    pub async fn execute(&self, cmd: EnqueueCommand) -> Result<JobId, EnqueueError> {
        validate_policy(&cmd.retry_policy)?;
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
