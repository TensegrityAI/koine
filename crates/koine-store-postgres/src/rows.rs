//! Row ↔ envelope mapping (ADR 0010 encoding over ADR 0012 columns).

use chrono::{DateTime, Utc};
use koine_application::ports::EventStoreError;
use koine_domain::{CorrelationId, EventEnvelope, EventId, JobEvent, JobId};
use sqlx::Row as _;
use sqlx::postgres::PgRow;
use uuid::Uuid;

/// Maps any sqlx error into the port's backend error.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn db(e: sqlx::Error) -> EventStoreError {
    EventStoreError::Backend(format!("db: {e}"))
}

/// Rebuilds an envelope from an `event_store.events` row.
pub(crate) fn envelope_from_row(row: &PgRow) -> Result<EventEnvelope, EventStoreError> {
    let payload: serde_json::Value = row.try_get("payload").map_err(db)?;
    let event: JobEvent = serde_json::from_value(payload)
        .map_err(|e| EventStoreError::Backend(format!("payload decode: {e}")))?;
    Ok(EventEnvelope {
        event_id: EventId::new(row.try_get::<Uuid, _>("event_id").map_err(db)?),
        stream_id: JobId::new(row.try_get::<Uuid, _>("stream_id").map_err(db)?),
        version: u64::try_from(row.try_get::<i64, _>("version").map_err(db)?)
            .map_err(|_| EventStoreError::Backend("negative version".into()))?,
        recorded_at: row.try_get::<DateTime<Utc>, _>("recorded_at").map_err(db)?,
        correlation_id: CorrelationId::new(row.try_get::<Uuid, _>("correlation_id").map_err(db)?),
        causation_id: row
            .try_get::<Option<Uuid>, _>("causation_id")
            .map_err(db)?
            .map(EventId::new),
        traceparent: row.try_get("traceparent").map_err(db)?,
        schema_version: u16::try_from(row.try_get::<i16, _>("schema_version").map_err(db)?)
            .map_err(|_| EventStoreError::Backend("negative schema_version".into()))?,
        event,
    })
}
