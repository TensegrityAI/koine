//! Deterministic clock and id generator for rings 1–2 (and 1B's ring 3).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use koine_application::ports::{Clock, IdGenerator};
use koine_domain::{CorrelationId, EventId, JobId, LeaseId};
use uuid::Uuid;

/// Manually-advanced test clock.
pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    /// Starts the clock at `instant`.
    #[must_use]
    pub fn at(instant: DateTime<Utc>) -> Self {
        Self(Mutex::new(instant))
    }

    /// Moves time forward.
    pub fn advance(&self, by: Duration) {
        let delta = TimeDelta::from_std(by).unwrap_or(TimeDelta::MAX);
        match self.0.lock() {
            Ok(mut guard) => *guard += delta,
            Err(poisoned) => *poisoned.into_inner() += delta,
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        match self.0.lock() {
            Ok(guard) => *guard,
            Err(poisoned) => *poisoned.into_inner(),
        }
    }
}

/// Sequential deterministic ids: `seed` in the high bits, a counter below.
pub struct SeededIds {
    seed: u64,
    counter: AtomicU64,
}

impl SeededIds {
    /// A generator whose ids embed `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            counter: AtomicU64::new(1),
        }
    }

    fn next(&self) -> Uuid {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        Uuid::from_u128((u128::from(self.seed) << 64) | u128::from(n))
    }
}

impl IdGenerator for SeededIds {
    fn job_id(&self) -> JobId {
        JobId::new(self.next())
    }
    fn event_id(&self) -> EventId {
        EventId::new(self.next())
    }
    fn lease_id(&self) -> LeaseId {
        LeaseId::new(self.next())
    }
    fn correlation_id(&self) -> CorrelationId {
        CorrelationId::new(self.next())
    }
    fn jitter_seed(&self) -> u64 {
        self.seed
    }
}
