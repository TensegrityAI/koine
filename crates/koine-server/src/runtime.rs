//! Production `Clock`/`IdGenerator` implementations (composition root).

use chrono::{DateTime, Utc};
use koine_application::ports::{Clock, IdGenerator};
use koine_domain::{CorrelationId, EventId, JobId, LeaseId};
use uuid::Uuid;

/// Wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// `UUIDv7` identity source (ADR 0010).
pub struct UuidV7Ids;

impl IdGenerator for UuidV7Ids {
    fn job_id(&self) -> JobId {
        JobId::new(Uuid::now_v7())
    }
    fn event_id(&self) -> EventId {
        EventId::new(Uuid::now_v7())
    }
    fn lease_id(&self) -> LeaseId {
        LeaseId::new(Uuid::now_v7())
    }
    fn correlation_id(&self) -> CorrelationId {
        CorrelationId::new(Uuid::now_v7())
    }
    fn jitter_seed(&self) -> u64 {
        // High-entropy per the port contract: fold both UUID halves.
        let bits = Uuid::now_v7().as_u128();
        #[allow(clippy::cast_possible_truncation)] // intentional fold of both halves
        {
            (bits as u64) ^ ((bits >> 64) as u64)
        }
    }
}
