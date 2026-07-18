//! Koiné domain layer: aggregates, domain events, state machines. No I/O, no async, no infra deps.

pub mod error;
pub mod ids;
pub mod queue;

pub use error::DomainError;
pub use ids::{CorrelationId, EventId, JobId, LeaseId, WorkerId};
pub use queue::{Priority, QueueName};
