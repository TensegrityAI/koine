//! Shared `EventSink` implementations for the composition root.

use koine_application::ports::{EventSink, SinkError};
use koine_domain::EventEnvelope;

/// Prints each delivered envelope's stream, version, and kind — the
/// placeholder sink both `dev-loop` and `serve` drain the transactional
/// outbox into until a real consumer arrives (phase 2B/3).
pub struct PrintingSink;

impl EventSink for PrintingSink {
    async fn deliver(&self, envelopes: &[EventEnvelope]) -> Result<(), SinkError> {
        for env in envelopes {
            println!(
                "  [outbox→sink] {} v{} {}",
                env.stream_id,
                env.version,
                env.event.kind()
            );
        }
        Ok(())
    }
}
