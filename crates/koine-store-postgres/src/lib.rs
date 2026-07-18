//! Koiné Postgres driven adapter: event store, transactional outbox, projections.

use sqlx::PgPool;

/// Embedded migrations (append-only files under `migrations/`).
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// Connects and runs migrations. The single entry point composition roots use.
///
/// # Errors
/// Connection or migration failure.
pub async fn connect_pool(url: &str) -> Result<PgPool, sqlx::Error> {
    let pool = PgPool::connect(url).await?;
    MIGRATOR.run(&pool).await.map_err(sqlx::Error::from)?;
    Ok(pool)
}

pub mod dispatcher;
pub mod relay;
mod rows;
pub mod store;

pub use dispatcher::PostgresDispatcher;
pub use relay::PostgresOutboxRelay;
pub use store::{PostgresEventStore, rebuild_dispatch};
