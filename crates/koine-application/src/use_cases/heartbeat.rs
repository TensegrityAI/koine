//! Lease keep-alive. Ephemeral by design: no event is written (ADR 0011-c).

use std::time::Duration;

use koine_domain::LeaseId;

use crate::ports::{DispatchError, Dispatcher};

/// Use case: extend a live lease.
pub struct Heartbeat<'a, D> {
    /// Dispatcher port.
    pub dispatcher: &'a D,
}

impl<D: Dispatcher> Heartbeat<'_, D> {
    /// Returns `false` when the lease is gone — the worker must stop.
    ///
    /// # Errors
    ///
    /// Fails if the dispatcher port fails.
    pub async fn execute(&self, lease: LeaseId, ttl: Duration) -> Result<bool, DispatchError> {
        self.dispatcher.extend_lease(lease, ttl).await
    }
}
