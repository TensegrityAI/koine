//! Envelope construction shared by all use cases.

use koine_domain::{CorrelationId, EventEnvelope, EventId, JobEvent, JobId, SCHEMA_VERSION};

use crate::ports::{Clock, IdGenerator};

/// Caller-supplied causal context for a command.
#[derive(Debug, Clone, Default)]
pub struct Lineage {
    /// Correlates this command's events with the caller's operation
    /// (`None` = the use case mints a fresh one where it starts a stream).
    pub correlation_id: Option<CorrelationId>,
    /// The event that caused this command, if any.
    pub causation_id: Option<EventId>,
    /// W3C trace context.
    pub traceparent: Option<String>,
}

/// Wraps domain events into envelopes with sequential versions after
/// `base_version`, one shared `recorded_at`/lineage, fresh event ids.
// Owned by value to match every planned call site (Tasks 9-12) exactly;
// fan-out over N events clones internally regardless of by-value vs by-ref.
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn wrap_events<G, C>(
    ids: &G,
    clock: &C,
    stream: JobId,
    base_version: u64,
    correlation_id: CorrelationId,
    causation_id: Option<EventId>,
    traceparent: Option<String>,
    events: Vec<JobEvent>,
) -> Vec<EventEnvelope>
where
    G: IdGenerator + ?Sized,
    C: Clock + ?Sized,
{
    let recorded_at = clock.now();
    let mut version = base_version;
    events
        .into_iter()
        .map(|event| {
            version += 1;
            EventEnvelope {
                event_id: ids.event_id(),
                stream_id: stream,
                version,
                recorded_at,
                correlation_id,
                causation_id,
                traceparent: traceparent.clone(),
                schema_version: SCHEMA_VERSION,
                event,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{Clock, IdGenerator};
    use chrono::{DateTime, TimeZone, Utc};
    use koine_domain::{CorrelationId, EventId, JobEvent, JobId, LeaseId};
    use std::sync::atomic::{AtomicU64, Ordering};
    use uuid::Uuid;

    struct TestIds(AtomicU64);
    impl TestIds {
        fn next(&self) -> Uuid {
            Uuid::from_u128(u128::from(self.0.fetch_add(1, Ordering::Relaxed)))
        }
    }
    impl IdGenerator for TestIds {
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
            7
        }
    }

    struct TestClock;
    impl Clock for TestClock {
        fn now(&self) -> DateTime<Utc> {
            Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
                .single()
                .expect("ts")
        }
    }

    #[test]
    fn wraps_events_with_sequential_versions_and_shared_lineage() {
        let ids = TestIds(AtomicU64::new(1));
        let clock = TestClock;
        let stream = JobId::new(Uuid::from_u128(500));
        let correlation = CorrelationId::new(Uuid::from_u128(600));
        let envelopes = wrap_events(
            &ids,
            &clock,
            stream,
            4,
            correlation,
            None,
            Some("00-abc-def-01".into()),
            vec![JobEvent::Suspended, JobEvent::Resumed],
        );
        assert_eq!(envelopes.len(), 2);
        assert_eq!(envelopes[0].version, 5);
        assert_eq!(envelopes[1].version, 6);
        assert_eq!(envelopes[0].correlation_id, correlation);
        assert_eq!(envelopes[1].traceparent.as_deref(), Some("00-abc-def-01"));
        assert_ne!(envelopes[0].event_id, envelopes[1].event_id);
    }
}
