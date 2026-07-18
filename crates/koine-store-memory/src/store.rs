//! In-memory `EventStore` honoring the ADR 0006/0011 contract exactly as the
//! Postgres adapter will: append and dispatch-index update are one atomic
//! step (here: one mutex hold; there: one transaction).

use std::collections::HashMap;
use std::future::Future;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use koine_application::ports::{EventStore, EventStoreError};
use koine_domain::{EventEnvelope, Job, JobId, JobState, LeaseId, Priority, QueueName, WorkerId};

/// A live lease as the dispatch index sees it (deadline is ephemeral —
/// heartbeats move it without events, ADR 0011-c).
#[derive(Debug, Clone)]
pub(crate) struct LeaseState {
    #[allow(dead_code)]
    pub(crate) lease: LeaseId,
    #[allow(dead_code)]
    pub(crate) worker: WorkerId,
    #[allow(dead_code)]
    pub(crate) expires_at: DateTime<Utc>,
}

/// One dispatchable (or leased) job in the index.
#[derive(Debug, Clone)]
pub(crate) struct DispatchEntry {
    #[allow(dead_code)]
    pub(crate) queue: QueueName,
    #[allow(dead_code)]
    pub(crate) priority: Priority,
    pub(crate) seq: u64,
    #[allow(dead_code)]
    pub(crate) not_before: Option<DateTime<Utc>>,
    #[allow(dead_code)]
    pub(crate) lease: Option<LeaseState>,
}

#[derive(Default)]
pub(crate) struct Inner {
    pub(crate) streams: HashMap<JobId, Vec<EventEnvelope>>,
    pub(crate) index: HashMap<JobId, DispatchEntry>,
    pub(crate) seq: u64,
}

/// In-memory event store plus dispatch index.
#[derive(Default)]
pub struct InMemoryEventStore {
    pub(crate) inner: Mutex<Inner>,
}

impl InMemoryEventStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn locked<T>(
        &self,
        f: impl FnOnce(&mut Inner) -> Result<T, EventStoreError>,
    ) -> Result<T, EventStoreError> {
        match self.inner.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(_) => Err(EventStoreError::Backend("store mutex poisoned".into())),
        }
    }

    pub(crate) fn append_locked(
        inner: &mut Inner,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> Result<(), EventStoreError> {
        // Compute current length WITHOUT inserting
        let current = u64::try_from(inner.streams.get(&stream).map_or(0, Vec::len))
            .map_err(|_| EventStoreError::Backend("stream too long".into()))?;

        // Run validations BEFORE touching the map
        if current != expected_version {
            return Err(EventStoreError::VersionConflict {
                stream,
                expected: expected_version,
            });
        }
        let mut next = current;
        for envelope in &envelopes {
            next += 1;
            if envelope.version != next || envelope.stream_id != stream {
                return Err(EventStoreError::Backend(format!(
                    "malformed envelope batch for {stream}"
                )));
            }
        }

        // Only on success: fold the combined batch BEFORE any map mutation,
        // so a version-sequential but domain-illegal batch never persists.
        let mut combined = inner.streams.get(&stream).cloned().unwrap_or_default();
        combined.extend(envelopes);
        let folded = Job::from_events(&combined)
            .map_err(|e| EventStoreError::Backend(format!("batch does not fold: {e}")))?;
        inner.streams.insert(stream, combined);
        Self::project_locked(inner, &folded);
        Ok(())
    }

    /// Re-derives the job's dispatch entry from its folded state — the index
    /// is a rebuildable projection, updated atomically with every append.
    pub(crate) fn project_locked(inner: &mut Inner, job: &Job) {
        let seq = inner.index.get(&job.id).map_or_else(
            || {
                inner.seq += 1;
                inner.seq
            },
            |entry| entry.seq,
        );
        match &job.state {
            JobState::Pending { not_before } => {
                inner.index.insert(
                    job.id,
                    DispatchEntry {
                        queue: job.queue.clone(),
                        priority: job.priority,
                        seq,
                        not_before: *not_before,
                        lease: None,
                    },
                );
            }
            JobState::Leased {
                worker,
                lease,
                expires_at,
            }
            | JobState::Running {
                worker,
                lease,
                expires_at,
            } => {
                inner.index.insert(
                    job.id,
                    DispatchEntry {
                        queue: job.queue.clone(),
                        priority: job.priority,
                        seq,
                        not_before: None,
                        lease: Some(LeaseState {
                            lease: *lease,
                            worker: worker.clone(),
                            expires_at: *expires_at,
                        }),
                    },
                );
            }
            JobState::Succeeded
            | JobState::Parked { .. }
            | JobState::Cancelled
            | JobState::Suspended
            | JobState::AwaitingApproval { .. } => {
                inner.index.remove(&job.id);
            }
        }
    }
}

impl EventStore for InMemoryEventStore {
    fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> impl Future<Output = Result<(), EventStoreError>> + Send {
        let result =
            self.locked(|inner| Self::append_locked(inner, stream, expected_version, envelopes));
        async move { result }
    }

    fn load(
        &self,
        stream: JobId,
    ) -> impl Future<Output = Result<Vec<EventEnvelope>, EventStoreError>> + Send {
        let result = self.locked(|inner| {
            inner
                .streams
                .get(&stream)
                .cloned()
                .ok_or(EventStoreError::StreamNotFound(stream))
        });
        async move { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FixedClock, SeededIds};
    use chrono::{TimeZone, Utc};
    use koine_application::{ports::EventStore, wrap_events};
    use koine_domain::{Job, JobEvent, Priority, QueueName, RetryPolicy};

    fn clock() -> FixedClock {
        FixedClock::at(
            Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
                .single()
                .expect("ts"),
        )
    }

    fn enqueue_envelopes(
        ids: &SeededIds,
        clock: &FixedClock,
    ) -> (koine_domain::JobId, Vec<koine_domain::EventEnvelope>) {
        use koine_application::ports::IdGenerator;
        let stream = ids.job_id();
        let correlation = ids.correlation_id();
        let event = Job::initial_event(
            QueueName::new("default").expect("q"),
            serde_json::json!({"n": 1}),
            Priority(0),
            RetryPolicy::default(),
            None,
        );
        (
            stream,
            wrap_events(ids, clock, stream, 0, correlation, None, None, vec![event]),
        )
    }

    #[tokio::test]
    async fn appends_and_loads_a_stream() {
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(1);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store
            .append(stream, 0, envelopes.clone())
            .await
            .expect("append");
        let loaded = store.load(stream).await.expect("load");
        assert_eq!(loaded, envelopes);
    }

    #[tokio::test]
    async fn rejects_version_conflicts() {
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(1);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store
            .append(stream, 0, envelopes.clone())
            .await
            .expect("append");
        let err = store
            .append(stream, 0, envelopes)
            .await
            .expect_err("conflict");
        assert!(matches!(
            err,
            koine_application::EventStoreError::VersionConflict { expected: 0, .. }
        ));
    }

    #[tokio::test]
    async fn load_of_unknown_stream_is_not_found() {
        use koine_application::ports::IdGenerator;
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(2);
        let err = store.load(ids.job_id()).await.expect_err("missing");
        assert!(matches!(
            err,
            koine_application::EventStoreError::StreamNotFound(_)
        ));
    }

    #[tokio::test]
    async fn append_maintains_the_dispatch_index() {
        use koine_application::ports::IdGenerator;
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(3);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        store.append(stream, 0, envelopes).await.expect("append");
        {
            let inner = store.inner.lock().expect("lock");
            let entry = inner.index.get(&stream).expect("indexed after enqueue");
            assert!(entry.lease.is_none());
        }
        // cancel ⇒ removed from the index atomically with the append
        let correlation = ids.correlation_id();
        let cancel = wrap_events(
            &ids,
            &clock,
            stream,
            1,
            correlation,
            None,
            None,
            vec![JobEvent::Cancelled { reason: None }],
        );
        store
            .append(stream, 1, cancel)
            .await
            .expect("append cancel");
        let inner = store.inner.lock().expect("lock");
        assert!(
            !inner.index.contains_key(&stream),
            "terminal ⇒ undispatchable"
        );
    }

    #[tokio::test]
    async fn fold_rejected_append_leaves_no_trace() {
        use koine_application::ports::IdGenerator;
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(5);
        let clock = clock();
        let stream = ids.job_id();
        let correlation = ids.correlation_id();
        // version-sequential but domain-illegal: a stream cannot start with `suspended`
        let bad = koine_application::wrap_events(
            &ids,
            &clock,
            stream,
            0,
            correlation,
            None,
            None,
            vec![koine_domain::JobEvent::Suspended],
        );
        let err = store
            .append(stream, 0, bad)
            .await
            .expect_err("must not fold");
        assert!(matches!(
            err,
            koine_application::EventStoreError::Backend(_)
        ));
        let err = store.load(stream).await.expect_err("no residue");
        assert!(matches!(
            err,
            koine_application::EventStoreError::StreamNotFound(_)
        ));
    }

    #[tokio::test]
    async fn failed_append_leaves_no_phantom_stream() {
        let store = InMemoryEventStore::new();
        let ids = SeededIds::new(4);
        let clock = clock();
        let (stream, envelopes) = enqueue_envelopes(&ids, &clock);
        // wrong expected_version against a never-seen stream must reject...
        let err = store
            .append(stream, 5, envelopes)
            .await
            .expect_err("conflict");
        assert!(matches!(
            err,
            koine_application::EventStoreError::VersionConflict { expected: 5, .. }
        ));
        // ...and must NOT materialize an empty stream
        let err = store.load(stream).await.expect_err("no phantom stream");
        assert!(matches!(
            err,
            koine_application::EventStoreError::StreamNotFound(_)
        ));
    }
}
