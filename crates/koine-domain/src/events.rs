//! The v1 event taxonomy (spec §3) plus reserved durable-execution kinds,
//! and the storage envelope (ADR 0010). Kind strings are wire/storage
//! contract — never rename.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ids::{CorrelationId, EventId, JobId, LeaseId, WorkerId},
    queue::{Priority, QueueName},
    retry::RetryPolicy,
};

/// Envelope schema version (bumped only on envelope-shape changes).
pub const SCHEMA_VERSION: u16 = 1;

/// Structured failure info reported by a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobError {
    /// Machine-readable error class (worker-defined).
    pub kind: String,
    /// Human-readable message.
    pub message: String,
    /// Optional stacktrace.
    #[serde(default)]
    pub stacktrace: Option<String>,
    /// Whether retrying can plausibly succeed.
    pub retryable: bool,
}

/// Why a job was parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParkReason {
    /// The retry policy's attempts were exhausted.
    RetriesExhausted,
    /// The worker reported the error as not retryable.
    NonRetryableError,
    /// A human denied a requested approval (reserved — phase 5).
    ApprovalDenied,
}

/// Outcome a late-acking worker reported after its lease had expired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportedOutcome {
    /// The worker claims the job succeeded.
    Succeeded,
    /// The worker claims the job failed.
    Failed,
}

/// Everything that can happen to a job. Serde-internally-tagged: the
/// `snake_case` tag is the canonical kind string (`kind()`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JobEvent {
    /// A new job entered the system (always version 1 of its stream).
    Enqueued {
        /// Destination queue.
        queue: QueueName,
        /// Opaque worker payload.
        payload: Value,
        /// Dispatch priority.
        priority: Priority,
        /// Retry policy fixed at enqueue time.
        retry_policy: RetryPolicy,
        /// Earliest dispatch time (`None` = immediately eligible).
        #[serde(default)]
        not_before: Option<DateTime<Utc>>,
    },
    /// A worker acquired a lease.
    Leased {
        /// The claiming worker.
        worker: WorkerId,
        /// The lease grant.
        lease: LeaseId,
        /// Deadline unless extended by heartbeats (ephemeral extensions —
        /// ADR 0011).
        expires_at: DateTime<Utc>,
    },
    /// The worker began executing.
    Started {
        /// The executing worker.
        worker: WorkerId,
    },
    /// Terminal success.
    Succeeded {
        /// Optional worker-reported result.
        #[serde(default)]
        result: Option<Value>,
    },
    /// An attempt failed (retry decision follows as its own event).
    Failed {
        /// Structured error.
        error: JobError,
        /// The attempt number that failed (1-based).
        attempt: u32,
    },
    /// The lease deadline passed without an ack — the worker is presumed dead.
    LeaseExpired {
        /// The expired lease.
        lease: LeaseId,
    },
    /// A retry was scheduled with backoff.
    RetryScheduled {
        /// The attempt count already completed.
        attempt: u32,
        /// Earliest re-dispatch time.
        not_before: DateTime<Utc>,
    },
    /// Dead but repairable, with full history retained.
    Parked {
        /// Why.
        reason: ParkReason,
    },
    /// Terminal cancellation by an operator or agent.
    Cancelled {
        /// Optional reason.
        #[serde(default)]
        reason: Option<String>,
    },
    /// An ack arrived after the lease expired — recorded, never discarded
    /// (spec §3: information is never lost).
    LateAckConflict {
        /// The late worker.
        worker: WorkerId,
        /// The lease it thought it held.
        lease: LeaseId,
        /// What it reported.
        reported: ReportedOutcome,
    },
    /// The job crossed a stall threshold (no progress within its window).
    /// Informational record; produced by phase 2's heartbeat mechanics.
    Stalled,
    /// Reserved (phase 5): a journaled side-effect result.
    CheckpointRecorded {
        /// Step key (unique per job).
        key: String,
        /// Journaled result.
        data: Value,
    },
    /// Reserved (phase 5): external signal delivered to the job.
    SignalReceived {
        /// Signal name.
        name: String,
        /// Signal payload.
        data: Value,
    },
    /// Reserved (phase 5): human-in-the-loop approval requested.
    ApprovalRequested {
        /// Approval key.
        key: String,
    },
    /// Reserved (phase 5): approval granted.
    ApprovalGranted {
        /// Approval key.
        key: String,
        /// Who granted it.
        approver: String,
    },
    /// Reserved (phase 5): approval denied.
    ApprovalDenied {
        /// Approval key.
        key: String,
        /// Who denied it.
        approver: String,
    },
    /// Reserved (phase 5): execution suspended.
    Suspended,
    /// Reserved (phase 5): execution resumed.
    Resumed,
    /// Reserved (phase 5): operator repaired the job; it continues from its
    /// last checkpoint with history preserved.
    Repaired {
        /// Replacement payload, if the input was the problem.
        #[serde(default)]
        new_payload: Option<Value>,
        /// Operator note.
        note: String,
    },
}

impl JobEvent {
    /// Canonical kind string — identical to the serde tag.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Enqueued { .. } => "enqueued",
            Self::Leased { .. } => "leased",
            Self::Started { .. } => "started",
            Self::Succeeded { .. } => "succeeded",
            Self::Failed { .. } => "failed",
            Self::LeaseExpired { .. } => "lease_expired",
            Self::RetryScheduled { .. } => "retry_scheduled",
            Self::Parked { .. } => "parked",
            Self::Cancelled { .. } => "cancelled",
            Self::LateAckConflict { .. } => "late_ack_conflict",
            Self::Stalled => "stalled",
            Self::CheckpointRecorded { .. } => "checkpoint_recorded",
            Self::SignalReceived { .. } => "signal_received",
            Self::ApprovalRequested { .. } => "approval_requested",
            Self::ApprovalGranted { .. } => "approval_granted",
            Self::ApprovalDenied { .. } => "approval_denied",
            Self::Suspended => "suspended",
            Self::Resumed => "resumed",
            Self::Repaired { .. } => "repaired",
        }
    }
}

/// The unit of storage and truth: one event plus its lineage (ADR 0010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventEnvelope {
    /// This event's identity.
    pub event_id: EventId,
    /// The job (= stream) this belongs to.
    pub stream_id: JobId,
    /// 1-based position in the stream (optimistic-concurrency token).
    pub version: u64,
    /// Broker-side record time.
    pub recorded_at: DateTime<Utc>,
    /// Correlates all events of one logical operation.
    pub correlation_id: CorrelationId,
    /// The event that caused this one, if known.
    #[serde(default)]
    pub causation_id: Option<EventId>,
    /// W3C trace context of the causing operation.
    #[serde(default)]
    pub traceparent: Option<String>,
    /// Envelope schema version.
    pub schema_version: u16,
    /// The event itself.
    pub event: JobEvent,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ids::*, queue::*, retry::RetryPolicy};
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn sample_events() -> Vec<JobEvent> {
        let worker = WorkerId::new("w1").expect("valid");
        let lease = LeaseId::new(Uuid::from_u128(2));
        let t = Utc
            .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
            .single()
            .expect("valid ts");
        vec![
            JobEvent::Enqueued {
                queue: QueueName::new("default").expect("valid"),
                payload: serde_json::json!({"n": 1}),
                priority: Priority(5),
                retry_policy: RetryPolicy::default(),
                not_before: None,
            },
            JobEvent::Leased {
                worker: worker.clone(),
                lease,
                expires_at: t,
            },
            JobEvent::Started {
                worker: worker.clone(),
            },
            JobEvent::Succeeded {
                result: Some(serde_json::json!("ok")),
            },
            JobEvent::Failed {
                error: JobError {
                    kind: "io".into(),
                    message: "boom".into(),
                    stacktrace: None,
                    retryable: true,
                },
                attempt: 1,
            },
            JobEvent::LeaseExpired { lease },
            JobEvent::RetryScheduled {
                attempt: 2,
                not_before: t,
            },
            JobEvent::Parked {
                reason: ParkReason::RetriesExhausted,
            },
            JobEvent::Cancelled {
                reason: Some("operator".into()),
            },
            JobEvent::LateAckConflict {
                worker,
                lease,
                reported: ReportedOutcome::Succeeded,
            },
            JobEvent::Stalled,
            JobEvent::CheckpointRecorded {
                key: "step-1".into(),
                data: serde_json::json!(1),
            },
            JobEvent::SignalReceived {
                name: "resume".into(),
                data: serde_json::json!(null),
            },
            JobEvent::ApprovalRequested {
                key: "deploy".into(),
            },
            JobEvent::ApprovalGranted {
                key: "deploy".into(),
                approver: "kael".into(),
            },
            JobEvent::ApprovalDenied {
                key: "deploy".into(),
                approver: "kael".into(),
            },
            JobEvent::Suspended,
            JobEvent::Resumed,
            JobEvent::Repaired {
                new_payload: None,
                note: "fixed input".into(),
            },
        ]
    }

    #[test]
    fn serde_tag_matches_kind_for_every_variant() {
        for ev in sample_events() {
            let json = serde_json::to_value(&ev).expect("serialize");
            assert_eq!(json["type"], ev.kind(), "tag/kind drift on {ev:?}");
            let back: JobEvent = serde_json::from_value(json).expect("deserialize");
            assert_eq!(back, ev);
        }
    }

    #[test]
    fn envelope_round_trips() {
        let env = EventEnvelope {
            event_id: EventId::new(Uuid::from_u128(10)),
            stream_id: JobId::new(Uuid::from_u128(11)),
            version: 1,
            recorded_at: Utc
                .with_ymd_and_hms(2026, 7, 18, 12, 0, 0)
                .single()
                .expect("ts"),
            correlation_id: CorrelationId::new(Uuid::from_u128(12)),
            causation_id: None,
            traceparent: Some("00-11111111111111111111111111111111-2222222222222222-01".into()),
            schema_version: SCHEMA_VERSION,
            event: JobEvent::Suspended,
        };
        let json = serde_json::to_string(&env).expect("serialize");
        let back: EventEnvelope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, env);
    }

    #[test]
    fn envelope_deserializes_without_optional_lineage() {
        // additive-evolution guarantee: absent optional fields default.
        let json = serde_json::json!({
            "event_id": Uuid::from_u128(1),
            "stream_id": Uuid::from_u128(2),
            "version": 1,
            "recorded_at": "2026-07-18T12:00:00Z",
            "correlation_id": Uuid::from_u128(3),
            "schema_version": 1,
            "event": {"type": "suspended"}
        });
        let env: EventEnvelope = serde_json::from_value(json).expect("deserialize");
        assert_eq!(env.causation_id, None);
        assert_eq!(env.traceparent, None);
    }
}
