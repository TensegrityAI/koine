//! `PostgresEventStore`: append, dispatch projection, and outbox in ONE
//! transaction (ADRs 0006/0011/0012). A failed transaction leaves nothing —
//! the same contract the in-memory store proves with one mutex hold.

use koine_application::ports::{EventStore, EventStoreError};
use koine_domain::{EventEnvelope, Job, JobId, JobState};
use sqlx::{PgPool, Postgres, Transaction};

use crate::rows::{db, envelope_from_row};

/// Event store over Postgres.
pub struct PostgresEventStore {
    pool: PgPool,
}

impl PostgresEventStore {
    /// Wraps a migrated pool (see [`crate::connect_pool`]).
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn is_version_conflict(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::Database(db_err)
            if db_err.code().as_deref() == Some("23505")
                && db_err.constraint() == Some("events_stream_version_unique")
    )
}

/// Loads a stream's envelopes inside the transaction, version order.
pub(crate) async fn load_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    stream: JobId,
) -> Result<Vec<EventEnvelope>, EventStoreError> {
    let rows = sqlx::query(
        "SELECT stream_id, version, event_id, event_type, schema_version, payload, \
         correlation_id, causation_id, traceparent, recorded_at \
         FROM event_store.events WHERE stream_id = $1 ORDER BY version",
    )
    .bind(stream.as_uuid())
    .fetch_all(&mut **tx)
    .await
    .map_err(db)?;
    rows.iter().map(envelope_from_row).collect()
}

/// The append composite: version check, event + outbox inserts, fold
/// validation, dispatch projection — caller owns commit/rollback.
pub(crate) async fn append_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    stream: JobId,
    expected_version: u64,
    envelopes: &[EventEnvelope],
) -> Result<Job, EventStoreError> {
    let current: Option<i64> =
        sqlx::query_scalar("SELECT max(version) FROM event_store.events WHERE stream_id = $1")
            .bind(stream.as_uuid())
            .fetch_one(&mut **tx)
            .await
            .map_err(db)?;
    let current = u64::try_from(current.unwrap_or(0))
        .map_err(|_| EventStoreError::Backend("negative stream version".into()))?;
    if current != expected_version {
        return Err(EventStoreError::VersionConflict {
            stream,
            expected: expected_version,
        });
    }
    let mut next = current;
    for envelope in envelopes {
        next += 1;
        if envelope.version != next || envelope.stream_id != stream {
            return Err(EventStoreError::Backend(format!(
                "malformed envelope batch for {stream}"
            )));
        }
    }
    for envelope in envelopes {
        let payload = serde_json::to_value(&envelope.event)
            .map_err(|e| EventStoreError::Backend(format!("payload encode: {e}")))?;
        let inserted = sqlx::query(
            "INSERT INTO event_store.events \
             (stream_id, version, event_id, event_type, schema_version, payload, \
              correlation_id, causation_id, traceparent, recorded_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(stream.as_uuid())
        .bind(
            i64::try_from(envelope.version)
                .map_err(|_| EventStoreError::Backend("version exceeds i64".into()))?,
        )
        .bind(envelope.event_id.as_uuid())
        .bind(envelope.event.kind())
        .bind(
            i16::try_from(envelope.schema_version)
                .map_err(|_| EventStoreError::Backend("schema_version exceeds i16".into()))?,
        )
        .bind(&payload)
        .bind(envelope.correlation_id.as_uuid())
        .bind(envelope.causation_id.map(|c| c.as_uuid()))
        .bind(envelope.traceparent.as_deref())
        .bind(envelope.recorded_at)
        .execute(&mut **tx)
        .await;
        match inserted {
            Ok(_) => {}
            Err(e) if is_version_conflict(&e) => {
                return Err(EventStoreError::VersionConflict {
                    stream,
                    expected: expected_version,
                });
            }
            Err(e) => return Err(db(e)),
        }
        let envelope_json = serde_json::to_value(envelope)
            .map_err(|e| EventStoreError::Backend(format!("envelope encode: {e}")))?;
        sqlx::query(
            "INSERT INTO event_store.outbox (event_id, stream_id, envelope) \
             VALUES ($1, $2, $3)",
        )
        .bind(envelope.event_id.as_uuid())
        .bind(stream.as_uuid())
        .bind(&envelope_json)
        .execute(&mut **tx)
        .await
        .map_err(db)?;
    }
    let stream_envelopes = load_in_tx(tx, stream).await?;
    let job = Job::from_events(&stream_envelopes)
        .map_err(|e| EventStoreError::Backend(format!("stream does not fold: {e}")))?;
    project_in_tx(tx, &job).await?;
    Ok(job)
}

/// Re-derives the job's dispatch row from folded state (rebuildable
/// projection — identical contract to the memory store's `project_locked`).
pub(crate) async fn project_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    job: &Job,
) -> Result<(), EventStoreError> {
    match &job.state {
        JobState::Pending { not_before } => {
            sqlx::query(
                "INSERT INTO event_store.dispatch_queue \
                 (job_id, queue, priority, not_before) VALUES ($1, $2, $3, $4) \
                 ON CONFLICT (job_id) DO UPDATE SET \
                 queue = EXCLUDED.queue, priority = EXCLUDED.priority, \
                 not_before = EXCLUDED.not_before, \
                 lease_id = NULL, worker_id = NULL, lease_expires_at = NULL",
            )
            .bind(job.id.as_uuid())
            .bind(job.queue.as_str())
            .bind(job.priority.0)
            .bind(*not_before)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
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
            sqlx::query(
                "INSERT INTO event_store.dispatch_queue \
                 (job_id, queue, priority, not_before, lease_id, worker_id, lease_expires_at) \
                 VALUES ($1, $2, $3, NULL, $4, $5, $6) \
                 ON CONFLICT (job_id) DO UPDATE SET \
                 queue = EXCLUDED.queue, priority = EXCLUDED.priority, not_before = NULL, \
                 lease_id = EXCLUDED.lease_id, worker_id = EXCLUDED.worker_id, \
                 lease_expires_at = EXCLUDED.lease_expires_at",
            )
            .bind(job.id.as_uuid())
            .bind(job.queue.as_str())
            .bind(job.priority.0)
            .bind(lease.as_uuid())
            .bind(worker.as_str())
            .bind(*expires_at)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
        }
        JobState::Succeeded
        | JobState::Parked { .. }
        | JobState::Cancelled
        | JobState::Suspended
        | JobState::AwaitingApproval { .. } => {
            sqlx::query("DELETE FROM event_store.dispatch_queue WHERE job_id = $1")
                .bind(job.id.as_uuid())
                .execute(&mut **tx)
                .await
                .map_err(db)?;
        }
    }
    Ok(())
}

impl EventStore for PostgresEventStore {
    async fn append(
        &self,
        stream: JobId,
        expected_version: u64,
        envelopes: Vec<EventEnvelope>,
    ) -> Result<(), EventStoreError> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        append_in_tx(&mut tx, stream, expected_version, &envelopes).await?;
        tx.commit().await.map_err(db)
    }

    async fn load(&self, stream: JobId) -> Result<Vec<EventEnvelope>, EventStoreError> {
        let rows = sqlx::query(
            "SELECT stream_id, version, event_id, event_type, schema_version, payload, \
             correlation_id, causation_id, traceparent, recorded_at \
             FROM event_store.events WHERE stream_id = $1 ORDER BY version",
        )
        .bind(stream.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        if rows.is_empty() {
            return Err(EventStoreError::StreamNotFound(stream));
        }
        rows.iter().map(envelope_from_row).collect()
    }
}
