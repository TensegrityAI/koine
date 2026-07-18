//! The sweep: converts expired leases into recorded history
//! (`lease_expired` + retry decision). The broker's heartbeat-side of the
//! crash-recovery guarantee.

use koine_domain::{DomainError, Job};
use thiserror::Error;

use crate::lineage::{lineage_of, wrap_events};
use crate::ports::{Clock, DispatchError, Dispatcher, EventStore, EventStoreError, IdGenerator};

/// Errors from the sweep.
#[derive(Debug, Error)]
pub enum SweepError {
    /// Store failure.
    #[error(transparent)]
    Store(#[from] EventStoreError),
    /// Dispatcher failure.
    #[error(transparent)]
    Dispatch(#[from] DispatchError),
    /// Domain rejection outside the expected race.
    #[error(transparent)]
    Domain(#[from] DomainError),
}

/// Use case: sweep expired leases.
pub struct SweepExpiredLeases<'a, S, D, G, C> {
    /// Event store port.
    pub store: &'a S,
    /// Dispatcher port.
    pub dispatcher: &'a D,
    /// Id source.
    pub ids: &'a G,
    /// Time source.
    pub clock: &'a C,
}

impl<S: EventStore, D: Dispatcher, G: IdGenerator, C: Clock> SweepExpiredLeases<'_, S, D, G, C> {
    /// Expires every overdue lease; returns how many jobs were swept.
    /// Races (a job acked between listing and folding, or a concurrent
    /// append) are skipped — the next sweep sees the truth.
    ///
    /// # Errors
    ///
    /// Fails if the store or dispatcher ports fail, or if domain logic
    /// rejects the expiry outside the expected race window.
    pub async fn execute(&self) -> Result<u32, SweepError> {
        let now = self.clock.now();
        let mut swept = 0;
        for job_id in self.dispatcher.expired(now).await? {
            let stream = self.store.load(job_id).await?;
            let job = Job::from_events(&stream)?;
            let Ok(events) = job.expire_lease(now, self.ids.jitter_seed()) else {
                continue; // already acked or otherwise moved on — not expired
            };
            let (correlation, causation, traceparent) = lineage_of(&stream);
            let envelopes = wrap_events(
                self.ids,
                self.clock,
                job.id,
                job.version,
                correlation,
                causation,
                traceparent,
                events,
            );
            match self.store.append(job.id, job.version, envelopes).await {
                Ok(()) => swept += 1,
                Err(EventStoreError::VersionConflict { .. }) => {} // lost the race: skip
                Err(other) => return Err(other.into()),
            }
        }
        Ok(swept)
    }
}
