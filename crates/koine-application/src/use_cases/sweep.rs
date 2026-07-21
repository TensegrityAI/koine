//! The sweep: asks the dispatcher to atomically retire expired leases until
//! none remain. The broker's heartbeat-side of the crash-recovery guarantee.

use thiserror::Error;

use crate::ports::{DispatchError, Dispatcher};

/// Errors from the sweep.
#[derive(Debug, Error)]
pub enum SweepError {
    /// Dispatcher failure.
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
}

/// Use case: sweep expired leases.
pub struct SweepExpiredLeases<'a, D> {
    /// Dispatcher port.
    pub dispatcher: &'a D,
}

impl<D: Dispatcher> SweepExpiredLeases<'_, D> {
    /// Expires every overdue lease; returns how many jobs were swept.
    ///
    /// # Errors
    ///
    /// Fails if the dispatcher cannot atomically retire the next lease.
    pub async fn execute(&self) -> Result<u32, SweepError> {
        let mut swept = 0;
        while self.dispatcher.retire_next_expired_lease().await?.is_some() {
            swept += 1;
        }
        Ok(swept)
    }
}
