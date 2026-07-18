//! Driven ports (spec §2). The composite-operation contracts come from
//! ADRs 0006 and 0011: adapters guarantee atomicity; use cases stay thin.

use std::future::Future;
use std::time::Duration;

use chrono::{DateTime, Utc};
use koine_domain::{CorrelationId, EventEnvelope, EventId, JobId, LeaseId, QueueName, WorkerId};
use serde_json::Value;
use thiserror::Error;

/// Errors from event-store adapters.
#[derive(Debug, Error)]
pub enum EventStoreError {
    /// Optimistic-concurrency conflict: the stream moved under the caller.
    #[error("version conflict on {stream}: expected {expected}")]
    VersionConflict {
        /// The stream that conflicted.
        stream: JobId,
        /// The version the caller expected to be current.
        expected: u64,
    },
    /// The stream does not exist.
    #[error("stream {0} not found")]
    StreamNotFound(JobId),
    /// Adapter/backend failure.
    #[error("backend: {0}")]
    Backend(String),
}

/// Errors from dispatcher adapters.
#[derive(Debug, Error)]
pub enum DispatchError {
    /// A store operation inside the composite failed.
    #[error(transparent)]
    Store(#[from] EventStoreError),
    /// Adapter/backend failure.
    #[error("backend: {0}")]
    Backend(String),
}

/// Append-only event log.
///
/// Contract (ADR 0006 / 0011-a): `append` synchronously and atomically
/// updates the dispatch index as part of the same operation — an appended
/// `enqueued` is immediately claimable, an appended terminal event
/// immediately undispatchable.
pub trait EventStore: Send + Sync {
    /// Appends pre-versioned envelopes. `expected_version` is the stream's
    /// current last version (0 for a new stream); envelopes must continue
    /// it sequentially.
    fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> impl Future<Output = Result<(), EventStoreError>> + Send;

    /// Loads a full stream in version order.
    fn load(
        &self,
        stream: JobId,
    ) -> impl Future<Output = Result<Vec<EventEnvelope>, EventStoreError>> + Send;
}

/// A job handed to a worker after a successful claim.
#[derive(Debug, Clone, PartialEq)]
pub struct LeasedJob {
    /// The claimed job.
    pub job_id: JobId,
    /// Its queue.
    pub queue: QueueName,
    /// Opaque worker payload.
    pub payload: Value,
    /// Completed attempts before this lease (0 = first try).
    pub attempt: u32,
    /// The lease grant to ack against.
    pub lease: LeaseId,
    /// Deadline unless extended by heartbeats.
    pub expires_at: DateTime<Utc>,
    /// Correlation carried from the job's lineage.
    pub correlation_id: CorrelationId,
    /// Trace context carried from the job's lineage.
    pub traceparent: Option<String>,
}

/// Atomic claim plus ephemeral lease bookkeeping.
///
/// Contract (ADR 0011-b/c): `lease_next` atomically selects the
/// highest-priority eligible job (priority desc, then enqueue order,
/// `not_before <= now`), produces `leased` via the domain aggregate, appends
/// it, and updates the index — all one transaction. `extend_lease` touches
/// only the ephemeral deadline; no event is written.
pub trait Dispatcher: Send + Sync {
    /// Claims the next eligible job on `queue` for `worker`, or `None`.
    fn lease_next(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> impl Future<Output = Result<Option<LeasedJob>, DispatchError>> + Send;

    /// Extends a live lease's deadline. Returns `false` if the lease is
    /// unknown or already expired (the worker must stop working).
    fn extend_lease(
        &self,
        lease: LeaseId,
        ttl: Duration,
    ) -> impl Future<Output = Result<bool, DispatchError>> + Send;

    /// Jobs whose lease deadline has passed as of `now` (sweep input).
    fn expired(
        &self,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<JobId>, DispatchError>> + Send;
}

/// Time source.
pub trait Clock: Send + Sync {
    /// Current instant.
    fn now(&self) -> DateTime<Utc>;
}

/// Identity and randomness source (`UUIDv7` in production adapters — ADR 0010).
pub trait IdGenerator: Send + Sync {
    /// New job id.
    fn job_id(&self) -> JobId;
    /// New event id.
    fn event_id(&self) -> EventId;
    /// New lease id.
    fn lease_id(&self) -> LeaseId;
    /// New correlation id.
    fn correlation_id(&self) -> CorrelationId;
    /// Seed for deterministic retry jitter.
    fn jitter_seed(&self) -> u64;
}
