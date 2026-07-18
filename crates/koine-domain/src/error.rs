//! Domain errors.

use thiserror::Error;

/// Errors produced by domain validation and state transitions.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    /// The event is not applicable in the aggregate's current state.
    #[error("illegal transition: event `{event}` in state `{state}`")]
    IllegalTransition {
        /// State name at the time of the attempt.
        state: &'static str,
        /// Event kind that was rejected.
        event: &'static str,
    },
    /// A queue name failed validation.
    #[error("invalid queue name: {reason}")]
    InvalidQueueName {
        /// Which rule was violated.
        reason: &'static str,
    },
    /// A worker id failed validation.
    #[error("invalid worker id: {reason}")]
    InvalidWorkerId {
        /// Which rule was violated.
        reason: &'static str,
    },
    /// An event stream did not start with `enqueued`.
    #[error("event stream must start with `enqueued`, got `{got}`")]
    StreamMustStartWithEnqueued {
        /// Kind of the offending first event.
        got: &'static str,
    },
    /// The command references a lease the job does not currently hold.
    #[error("lease mismatch")]
    LeaseMismatch,
    /// A lease TTL could not be represented.
    #[error("ttl out of range")]
    InvalidTtl,
    /// Envelope versions were not sequential when folding a stream.
    #[error("non-sequential version: expected {expected}, got {got}")]
    NonSequentialVersion {
        /// The version the fold expected next.
        expected: u64,
        /// The version found on the envelope.
        got: u64,
    },
}
