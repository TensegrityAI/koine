//! The `Job` aggregate: state is a fold over events (ADR 0004). Commands
//! validate; `apply` trusts recorded history and only rejects transitions
//! that could never have been legal.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::{
    error::DomainError,
    events::{EventEnvelope, JobEvent, ParkReason},
    ids::{JobId, LeaseId, WorkerId},
    queue::{Priority, QueueName},
    retry::RetryPolicy,
};

/// Resting states of a job (spec §3 state machine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    /// Eligible (or scheduled) for dispatch.
    Pending {
        /// Earliest dispatch time; `None` = eligible now.
        not_before: Option<DateTime<Utc>>,
    },
    /// Claimed by a worker, not yet started.
    Leased {
        /// Holder.
        worker: WorkerId,
        /// The grant.
        lease: LeaseId,
        /// Recorded deadline (ephemeral extensions live in the dispatch
        /// index — ADR 0011).
        expires_at: DateTime<Utc>,
    },
    /// Executing.
    Running {
        /// Holder.
        worker: WorkerId,
        /// The grant.
        lease: LeaseId,
        /// Recorded deadline.
        expires_at: DateTime<Utc>,
    },
    /// Terminal success.
    Succeeded,
    /// Dead but repairable (full history retained).
    Parked {
        /// Why.
        reason: ParkReason,
    },
    /// Terminal cancellation.
    Cancelled,
    /// Reserved (phase 5).
    Suspended,
    /// Reserved (phase 5).
    AwaitingApproval {
        /// Approval key being waited on.
        key: String,
    },
}

impl JobState {
    /// Stable state name for diagnostics and error messages.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Pending { .. } => "pending",
            Self::Leased { .. } => "leased",
            Self::Running { .. } => "running",
            Self::Succeeded => "succeeded",
            Self::Parked { .. } => "parked",
            Self::Cancelled => "cancelled",
            Self::Suspended => "suspended",
            Self::AwaitingApproval { .. } => "awaiting_approval",
        }
    }
}

/// The aggregate. Public fields: the aggregate is a value; invariants are
/// guarded by `from_events`/`apply`, not by hiding.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// Identity (= stream id).
    pub id: JobId,
    /// Destination queue.
    pub queue: QueueName,
    /// Dispatch priority.
    pub priority: Priority,
    /// Opaque worker payload.
    pub payload: Value,
    /// Retry policy fixed at enqueue (until repaired).
    pub retry_policy: RetryPolicy,
    /// Completed (failed or expired) attempts.
    pub attempt: u32,
    /// Current state.
    pub state: JobState,
    /// Last applied stream version.
    pub version: u64,
}

impl Job {
    /// The stream-opening event for a new job.
    #[must_use]
    pub const fn initial_event(
        queue: QueueName,
        payload: Value,
        priority: Priority,
        retry_policy: RetryPolicy,
        not_before: Option<DateTime<Utc>>,
    ) -> JobEvent {
        JobEvent::Enqueued {
            queue,
            payload,
            priority,
            retry_policy,
            not_before,
        }
    }

    /// Rebuilds a job by folding its recorded stream.
    ///
    /// # Errors
    ///
    /// Returns `StreamMustStartWithEnqueued` if the first event is not
    /// `Enqueued`, and `NonSequentialVersion` if event versions are not
    /// sequential. Calls `apply` which may return `IllegalTransition`.
    pub fn from_events(envelopes: &[EventEnvelope]) -> Result<Self, DomainError> {
        let Some((first, rest)) = envelopes.split_first() else {
            return Err(DomainError::StreamMustStartWithEnqueued { got: "nothing" });
        };
        let JobEvent::Enqueued {
            queue,
            payload,
            priority,
            retry_policy,
            not_before,
        } = &first.event
        else {
            return Err(DomainError::StreamMustStartWithEnqueued {
                got: first.event.kind(),
            });
        };
        if first.version != 1 {
            return Err(DomainError::NonSequentialVersion {
                expected: 1,
                got: first.version,
            });
        }
        let mut job = Self {
            id: first.stream_id,
            queue: queue.clone(),
            priority: *priority,
            payload: payload.clone(),
            retry_policy: retry_policy.clone(),
            attempt: 0,
            state: JobState::Pending {
                not_before: *not_before,
            },
            version: 1,
        };
        for envelope in rest {
            if envelope.version != job.version + 1 {
                return Err(DomainError::NonSequentialVersion {
                    expected: job.version + 1,
                    got: envelope.version,
                });
            }
            job.apply(&envelope.event)?;
        }
        Ok(job)
    }

    /// Applies one event, advancing the state machine. Rejections leave the
    /// aggregate untouched (version included).
    ///
    /// # Errors
    ///
    /// Returns `IllegalTransition` if the event is not valid for the current state.
    #[allow(clippy::too_many_lines)] // transition table: one function on purpose
    #[allow(clippy::match_same_arms)] // semantic grouping: informational events always return state unchanged
    pub fn apply(&mut self, event: &JobEvent) -> Result<(), DomainError> {
        use JobEvent as E;
        use JobState as S;
        let current = self.state.clone();
        let next = match (current, event) {
            (
                S::Pending { .. },
                E::Leased {
                    worker,
                    lease,
                    expires_at,
                },
            ) => S::Leased {
                worker: worker.clone(),
                lease: *lease,
                expires_at: *expires_at,
            },
            (
                S::Leased {
                    worker,
                    lease,
                    expires_at,
                },
                E::Started { .. },
            ) => S::Running {
                worker,
                lease,
                expires_at,
            },
            (S::Running { .. }, E::Succeeded { .. }) => S::Succeeded,
            (S::Running { .. }, E::Failed { attempt, .. }) => {
                self.attempt = *attempt;
                S::Pending { not_before: None }
            }
            (
                S::Pending { .. },
                E::RetryScheduled {
                    attempt,
                    not_before,
                },
            ) => {
                self.attempt = *attempt;
                S::Pending {
                    not_before: Some(*not_before),
                }
            }
            (S::Leased { .. } | S::Running { .. }, E::LeaseExpired { .. }) => {
                self.attempt += 1;
                S::Pending { not_before: None }
            }
            (S::Pending { .. }, E::Parked { reason }) => S::Parked { reason: *reason },
            (
                S::Pending { .. }
                | S::Leased { .. }
                | S::Running { .. }
                | S::Suspended
                | S::AwaitingApproval { .. }
                | S::Parked { .. },
                E::Cancelled { .. },
            ) => S::Cancelled,
            // A late ack is a pure record: legal in every state, changes none.
            (state, E::LateAckConflict { .. }) => state,
            (state @ S::Running { .. }, E::CheckpointRecorded { .. }) => state,
            (
                state @ (S::Pending { .. }
                | S::Leased { .. }
                | S::Running { .. }
                | S::Suspended
                | S::AwaitingApproval { .. }),
                E::SignalReceived { .. },
            ) => state,
            // Stall-threshold crossings are informational records (spec §3);
            // produced by phase 2's heartbeat mechanics.
            (state @ (S::Leased { .. } | S::Running { .. }), E::Stalled) => state,
            (S::Running { .. }, E::ApprovalRequested { key }) => {
                S::AwaitingApproval { key: key.clone() }
            }
            (S::AwaitingApproval { .. }, E::ApprovalGranted { .. }) => {
                S::Pending { not_before: None }
            }
            (S::AwaitingApproval { .. }, E::ApprovalDenied { .. }) => S::Parked {
                reason: ParkReason::ApprovalDenied,
            },
            (S::Pending { .. } | S::Running { .. }, E::Suspended) => S::Suspended,
            (S::Suspended, E::Resumed) => S::Pending { not_before: None },
            (S::Parked { .. }, E::Repaired { new_payload, .. }) => {
                if let Some(payload) = new_payload {
                    self.payload = payload.clone();
                }
                self.attempt = 0;
                S::Pending { not_before: None }
            }
            (state, ev) => {
                let rejected = DomainError::IllegalTransition {
                    state: state.name(),
                    event: ev.kind(),
                };
                self.state = state;
                return Err(rejected);
            }
        };
        self.state = next;
        self.version += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::*, ids::*, queue::*, retry::RetryPolicy};
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn ts() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("ts")
    }

    fn env(version: u64, event: JobEvent) -> EventEnvelope {
        EventEnvelope {
            event_id: EventId::new(Uuid::from_u128(u128::from(version))),
            stream_id: JobId::new(Uuid::from_u128(1)),
            version,
            recorded_at: ts(),
            correlation_id: CorrelationId::new(Uuid::from_u128(9)),
            causation_id: None,
            traceparent: None,
            schema_version: SCHEMA_VERSION,
            event,
        }
    }

    fn enqueued() -> JobEvent {
        Job::initial_event(
            QueueName::new("default").expect("q"),
            serde_json::json!({"k": 1}),
            Priority(3),
            RetryPolicy::default(),
            None,
        )
    }

    fn worker() -> WorkerId {
        WorkerId::new("w1").expect("w")
    }

    fn lease_id() -> LeaseId {
        LeaseId::new(Uuid::from_u128(50))
    }

    #[test]
    fn folds_the_happy_path() {
        let job = Job::from_events(&[
            env(1, enqueued()),
            env(
                2,
                JobEvent::Leased {
                    worker: worker(),
                    lease: lease_id(),
                    expires_at: ts(),
                },
            ),
            env(3, JobEvent::Started { worker: worker() }),
            env(4, JobEvent::Succeeded { result: None }),
        ])
        .expect("fold");
        assert_eq!(job.state, JobState::Succeeded);
        assert_eq!(job.version, 4);
        assert_eq!(job.attempt, 0);
    }

    #[test]
    fn stream_must_start_with_enqueued() {
        let err = Job::from_events(&[env(1, JobEvent::Suspended)]).expect_err("must fail");
        assert_eq!(
            err,
            DomainError::StreamMustStartWithEnqueued { got: "suspended" }
        );
    }

    #[test]
    fn rejects_non_sequential_versions() {
        let err = Job::from_events(&[env(1, enqueued()), env(3, JobEvent::Suspended)])
            .expect_err("must fail");
        assert_eq!(
            err,
            DomainError::NonSequentialVersion {
                expected: 2,
                got: 3
            }
        );
    }

    #[test]
    fn started_is_illegal_while_pending() {
        let mut job = Job::from_events(&[env(1, enqueued())]).expect("fold");
        let err = job
            .apply(&JobEvent::Started { worker: worker() })
            .expect_err("illegal");
        assert_eq!(
            err,
            DomainError::IllegalTransition {
                state: "pending",
                event: "started"
            }
        );
        assert_eq!(job.version, 1, "version must not advance on rejection");
    }

    #[test]
    fn failed_records_attempt_and_returns_to_pending() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(
                2,
                JobEvent::Leased {
                    worker: worker(),
                    lease: lease_id(),
                    expires_at: ts(),
                },
            ),
            env(3, JobEvent::Started { worker: worker() }),
        ])
        .expect("fold");
        job.apply(&JobEvent::Failed {
            error: JobError {
                kind: "k".into(),
                message: "m".into(),
                stacktrace: None,
                retryable: true,
            },
            attempt: 1,
        })
        .expect("apply failed");
        assert_eq!(job.attempt, 1);
        assert_eq!(job.state, JobState::Pending { not_before: None });
        job.apply(&JobEvent::RetryScheduled {
            attempt: 1,
            not_before: ts(),
        })
        .expect("apply retry");
        assert_eq!(
            job.state,
            JobState::Pending {
                not_before: Some(ts())
            }
        );
    }

    #[test]
    fn lease_expiry_increments_attempt() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(
                2,
                JobEvent::Leased {
                    worker: worker(),
                    lease: lease_id(),
                    expires_at: ts(),
                },
            ),
        ])
        .expect("fold");
        job.apply(&JobEvent::LeaseExpired { lease: lease_id() })
            .expect("apply");
        assert_eq!(job.attempt, 1);
        assert_eq!(job.state, JobState::Pending { not_before: None });
    }

    #[test]
    fn terminal_states_absorb_everything_except_late_ack() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(2, JobEvent::Cancelled { reason: None }),
        ])
        .expect("fold");
        assert!(job.apply(&JobEvent::Suspended).is_err());
        job.apply(&JobEvent::LateAckConflict {
            worker: worker(),
            lease: lease_id(),
            reported: ReportedOutcome::Failed,
        })
        .expect("late ack is a pure record");
        assert_eq!(job.state, JobState::Cancelled);
        assert_eq!(job.version, 3);
    }

    #[test]
    fn repair_resets_attempts_and_replaces_payload() {
        let mut job = Job::from_events(&[
            env(1, enqueued()),
            env(
                2,
                JobEvent::Leased {
                    worker: worker(),
                    lease: lease_id(),
                    expires_at: ts(),
                },
            ),
            env(3, JobEvent::LeaseExpired { lease: lease_id() }),
            env(
                4,
                JobEvent::Parked {
                    reason: ParkReason::RetriesExhausted,
                },
            ),
        ])
        .expect("fold");
        job.apply(&JobEvent::Repaired {
            new_payload: Some(serde_json::json!({"k": 2})),
            note: "fixed".into(),
        })
        .expect("repair");
        assert_eq!(job.attempt, 0);
        assert_eq!(job.payload, serde_json::json!({"k": 2}));
        assert_eq!(job.state, JobState::Pending { not_before: None });
    }
}
