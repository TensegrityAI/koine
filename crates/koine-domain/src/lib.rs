//! Koiné domain layer: aggregates, domain events, state machines. No I/O, no async, no infra deps.

pub mod error;
pub mod events;
pub mod ids;
pub mod queue;
pub mod retry;

pub use error::DomainError;
pub use events::{EventEnvelope, JobError, JobEvent, ParkReason, ReportedOutcome, SCHEMA_VERSION};
pub use ids::{CorrelationId, EventId, JobId, LeaseId, WorkerId};
pub use queue::{Priority, QueueName};
pub use retry::{RetryDecision, RetryPolicy};
