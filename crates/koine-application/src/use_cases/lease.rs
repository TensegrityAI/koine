//! Claiming work (thin over the `Dispatcher` port — the atomicity lives in
//! the adapter, ADR 0011).

use std::time::Duration;

use koine_domain::{QueueName, WorkerId};

use crate::ports::{DispatchError, Dispatcher, LeasedJob};

/// Use case: claim the next eligible job.
pub struct LeaseNextJob<'a, D> {
    /// Dispatcher port.
    pub dispatcher: &'a D,
}

impl<D: Dispatcher> LeaseNextJob<'_, D> {
    /// Claims for `worker` on `queue`, or returns `None` when drained.
    ///
    /// # Errors
    ///
    /// Fails if the dispatcher port fails.
    pub async fn execute(
        &self,
        queue: &QueueName,
        worker: &WorkerId,
        ttl: Duration,
    ) -> Result<Option<LeasedJob>, DispatchError> {
        self.dispatcher.lease_next(queue, worker, ttl).await
    }
}
