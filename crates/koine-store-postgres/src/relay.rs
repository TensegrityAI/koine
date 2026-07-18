//! `PostgresOutboxRelay`: claims ordered batches with SKIP LOCKED, delivers
//! to a sink, deletes on success (ADR 0012 — claim-delete, no positions).

use koine_application::ports::{EventSink, RelayError};
use koine_domain::EventEnvelope;
use sqlx::{PgPool, Row as _};

/// Single-instance outbox relay (concurrency arrives with phase-3 consumers).
pub struct PostgresOutboxRelay {
    pool: PgPool,
}

impl PostgresOutboxRelay {
    /// New relay over a migrated pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// One pass: claim up to `batch` rows in `outbox_seq` order, deliver,
    /// delete. Returns rows delivered. Sink failure rolls the claim back.
    ///
    /// # Errors
    /// [`RelayError::Sink`] when the sink rejects (batch redelivered later);
    /// [`RelayError::Backend`] on database failure.
    pub async fn relay_once<S: EventSink>(&self, sink: &S, batch: i64) -> Result<u32, RelayError> {
        let backend = |e: sqlx::Error| RelayError::Backend(format!("db: {e}"));
        let mut tx = self.pool.begin().await.map_err(backend)?;
        let rows = sqlx::query(
            "SELECT outbox_seq, envelope FROM event_store.outbox \
             ORDER BY outbox_seq LIMIT $1 FOR UPDATE SKIP LOCKED",
        )
        .bind(batch)
        .fetch_all(&mut *tx)
        .await
        .map_err(backend)?;
        if rows.is_empty() {
            return Ok(0);
        }
        let mut seqs: Vec<i64> = Vec::with_capacity(rows.len());
        let mut envelopes: Vec<EventEnvelope> = Vec::with_capacity(rows.len());
        for row in &rows {
            seqs.push(row.try_get("outbox_seq").map_err(backend)?);
            let value: serde_json::Value = row.try_get("envelope").map_err(backend)?;
            envelopes.push(
                serde_json::from_value(value)
                    .map_err(|e| RelayError::Backend(format!("envelope decode: {e}")))?,
            );
        }
        sink.deliver(&envelopes).await?;
        sqlx::query("DELETE FROM event_store.outbox WHERE outbox_seq = ANY($1)")
            .bind(&seqs)
            .execute(&mut *tx)
            .await
            .map_err(backend)?;
        tx.commit().await.map_err(backend)?;
        Ok(u32::try_from(envelopes.len()).unwrap_or(u32::MAX))
    }
}
