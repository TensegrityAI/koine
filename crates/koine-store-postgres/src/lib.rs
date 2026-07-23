//! Koiné Postgres driven adapter: event store, transactional outbox, projections.

use std::num::{NonZeroU32, NonZeroU64};
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

// 16 (up from sqlx's implicit 10) covers the serve loop's steady concurrent
// consumers — the sweep and relay tickers plus one connection per in-flight
// RPC handler — with headroom, now that the dispatch listener runs on its own
// dedicated size-one pool (`PgSignal::connect`) and no longer competes here.
const DEFAULT_MAX_CONNECTIONS: NonZeroU32 = match NonZeroU32::new(16) {
    Some(value) => value,
    None => unreachable!(),
};
const DEFAULT_ACQUIRE_TIMEOUT_MS: NonZeroU64 = match NonZeroU64::new(5_000) {
    Some(value) => value,
    None => unreachable!(),
};

/// Bounded connection-pool settings supplied by composition roots.
#[derive(Debug, Clone, Copy)]
pub struct PoolConfig {
    max_connections: NonZeroU32,
    acquire_timeout_ms: NonZeroU64,
}

impl PoolConfig {
    /// Creates pool settings whose type excludes unbounded zero values.
    #[must_use]
    pub const fn new(max_connections: NonZeroU32, acquire_timeout_ms: NonZeroU64) -> Self {
        Self {
            max_connections,
            acquire_timeout_ms,
        }
    }

    /// Returns the maximum number of connections retained by the pool.
    #[must_use]
    pub const fn max_connections(self) -> NonZeroU32 {
        self.max_connections
    }

    /// Returns how long pool acquisition may wait before failing.
    #[must_use]
    pub const fn acquire_timeout(self) -> Duration {
        Duration::from_millis(self.acquire_timeout_ms.get())
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_CONNECTIONS, DEFAULT_ACQUIRE_TIMEOUT_MS)
    }
}

/// Embedded migrations (append-only files under `migrations/`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// Connects and runs migrations. The single entry point composition roots use.
///
/// # Errors
/// Connection or migration failure.
pub async fn connect_pool(url: &str, config: PoolConfig) -> Result<PgPool, sqlx::Error> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections().get())
        .acquire_timeout(config.acquire_timeout())
        .connect(url)
        .await?;
    MIGRATOR.run(&pool).await.map_err(sqlx::Error::from)?;
    Ok(pool)
}

pub mod dispatcher;
pub mod presence;
pub mod relay;
mod rows;
pub mod signal;
pub mod store;

pub use dispatcher::PostgresDispatcher;
pub use presence::PgPresence;
pub use relay::PostgresOutboxRelay;
pub use signal::PgSignal;
pub use store::{PostgresEventStore, rebuild_dispatch};
