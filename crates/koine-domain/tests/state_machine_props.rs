//! Ring-1 property tests (testing-policy): whatever a client does through
//! commands, and whatever a store replays as events, the aggregate never
//! panics, never corrupts, and terminal states absorb.

#![allow(clippy::expect_used, clippy::duration_suboptimal_units)]

use std::time::Duration;

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use koine_domain::{CorrelationId, EventId, JobId};
use koine_domain::{
    DomainError, EventEnvelope, Job, JobError, JobEvent, JobState, LeaseId, ParkReason, Priority,
    QueueName, ReportedOutcome, RetryDecision, RetryPolicy, SCHEMA_VERSION, WorkerId,
};
use proptest::prelude::*;
use uuid::Uuid;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("ts")
}

fn envelope(version: u64, event: JobEvent) -> EventEnvelope {
    EventEnvelope {
        event_id: EventId::new(Uuid::from_u128(1000 + u128::from(version))),
        stream_id: JobId::new(Uuid::from_u128(1)),
        version,
        recorded_at: t0(),
        correlation_id: CorrelationId::new(Uuid::from_u128(2)),
        causation_id: None,
        traceparent: None,
        schema_version: SCHEMA_VERSION,
        event,
    }
}

fn fresh_job() -> Job {
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay: Duration::from_millis(10),
        max_delay: Duration::from_millis(50),
    };
    let initial = Job::initial_event(
        QueueName::new("q").expect("q"),
        serde_json::json!({}),
        Priority(0),
        policy,
        None,
    );
    Job::from_events(&[envelope(1, initial)]).expect("initial fold")
}

#[derive(Debug, Clone)]
enum Cmd {
    Lease,
    Start,
    Succeed,
    FailRetryable,
    FailFatal,
    Expire,
    Cancel,
    Advance(u32),
}

fn cmd_strategy() -> impl Strategy<Value = Cmd> {
    prop_oneof![
        Just(Cmd::Lease),
        Just(Cmd::Start),
        Just(Cmd::Succeed),
        Just(Cmd::FailRetryable),
        Just(Cmd::FailFatal),
        Just(Cmd::Expire),
        Just(Cmd::Cancel),
        (0u32..7200).prop_map(Cmd::Advance),
    ]
}

fn job_error(retryable: bool) -> JobError {
    JobError {
        kind: "k".into(),
        message: "m".into(),
        stacktrace: None,
        retryable,
    }
}

proptest! {
    /// Any interleaving of commands folds cleanly: commands only ever emit
    /// applicable events, attempt is monotonic, version counts applied
    /// events, terminal states absorb.
    #[test]
    fn command_sequences_never_corrupt(
        cmds in proptest::collection::vec(cmd_strategy(), 0..60),
        seed in any::<u64>(),
    ) {
        let mut job = fresh_job();
        let mut now = t0();
        let mut applied: u64 = 1;
        let worker = WorkerId::new("w").expect("w");
        let mut lease_counter: u128 = 100;
        let mut current_lease = LeaseId::new(Uuid::from_u128(lease_counter));

        for cmd in cmds {
            let events: Vec<JobEvent> = match cmd {
                Cmd::Advance(secs) => {
                    now += TimeDelta::seconds(i64::from(secs));
                    vec![]
                }
                Cmd::Lease => {
                    lease_counter += 1;
                    let lease = LeaseId::new(Uuid::from_u128(lease_counter));
                    match job.lease(worker.clone(), lease, now, Duration::from_secs(30)) {
                        Ok(ev) => {
                            current_lease = lease;
                            vec![ev]
                        }
                        Err(_) => vec![],
                    }
                }
                Cmd::Start => job.start(&worker).map(|ev| vec![ev]).unwrap_or_default(),
                Cmd::Succeed => {
                    job.succeed(current_lease, None).map(|ev| vec![ev]).unwrap_or_default()
                }
                Cmd::FailRetryable => {
                    job.fail(current_lease, job_error(true), now, seed).unwrap_or_default()
                }
                Cmd::FailFatal => {
                    job.fail(current_lease, job_error(false), now, seed).unwrap_or_default()
                }
                Cmd::Expire => job.expire_lease(now, seed).unwrap_or_default(),
                Cmd::Cancel => job.cancel(None).map(|ev| vec![ev]).unwrap_or_default(),
            };

            let attempt_before = job.attempt;
            for event in &events {
                job.apply(event).expect("commands only emit applicable events");
                applied += 1;
            }
            prop_assert!(job.attempt >= attempt_before, "attempt is monotonic");
            prop_assert_eq!(job.version, applied, "version counts applied events");

            if matches!(job.state, JobState::Succeeded | JobState::Cancelled) {
                prop_assert!(
                    job.lease(
                        worker.clone(),
                        LeaseId::new(Uuid::from_u128(999_999)),
                        now,
                        Duration::from_secs(1),
                    )
                    .is_err(),
                    "terminal states absorb commands"
                );
            }
        }
    }

    /// Replaying arbitrary (even nonsensical) event sequences never panics:
    /// each event either applies (version +1) or is rejected untouched.
    #[test]
    fn arbitrary_event_replay_never_panics(
        events in proptest::collection::vec(event_strategy(), 0..40),
    ) {
        let mut job = fresh_job();
        for event in events {
            let version_before = job.version;
            let state_before = job.state.clone();
            match job.apply(&event) {
                Ok(()) => prop_assert_eq!(job.version, version_before + 1),
                Err(DomainError::IllegalTransition { .. }) => {
                    prop_assert_eq!(job.version, version_before, "rejection must not advance");
                    prop_assert_eq!(&job.state, &state_before, "rejection must not mutate");
                }
                Err(other) => prop_assert!(false, "unexpected error: {other}"),
            }
        }
    }

    /// The retry policy's delay never exceeds its cap.
    #[test]
    fn retry_delay_respects_cap(attempt in 1u32..200, seed in any::<u64>()) {
        let policy = RetryPolicy {
            max_attempts: 200,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
        };
        if let RetryDecision::RetryAfter(delay) = policy.decide(attempt, seed) {
            prop_assert!(delay <= Duration::from_secs(60));
        }
    }
}

fn event_strategy() -> impl Strategy<Value = JobEvent> {
    let worker = WorkerId::new("w").expect("w");
    let lease = LeaseId::new(Uuid::from_u128(101));
    prop_oneof![
        Just(JobEvent::Leased {
            worker: worker.clone(),
            lease,
            expires_at: t0()
        }),
        Just(JobEvent::Started {
            worker: worker.clone()
        }),
        Just(JobEvent::Succeeded { result: None }),
        (1u32..5).prop_map(|attempt| JobEvent::Failed {
            error: job_error(true),
            attempt
        }),
        Just(JobEvent::LeaseExpired { lease }),
        (1u32..5).prop_map(|attempt| JobEvent::RetryScheduled {
            attempt,
            not_before: t0()
        }),
        Just(JobEvent::Parked {
            reason: ParkReason::RetriesExhausted
        }),
        Just(JobEvent::Cancelled { reason: None }),
        Just(JobEvent::LateAckConflict {
            worker,
            lease,
            reported: ReportedOutcome::Succeeded,
        }),
        Just(JobEvent::CheckpointRecorded {
            key: "s".into(),
            data: serde_json::json!(1)
        }),
        Just(JobEvent::ApprovalRequested { key: "a".into() }),
        Just(JobEvent::Suspended),
        Just(JobEvent::Resumed),
        Just(JobEvent::Repaired {
            new_payload: None,
            note: "n".into()
        }),
    ]
}
