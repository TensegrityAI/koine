//! Ring-3 harness: one throwaway Postgres container per test, real
//! migrations. Copied verbatim from
//! `koine-store-postgres/tests/support/mod.rs` (phase-2B dedup follow-up:
//! this duplication should collapse into one shared test-support crate once
//! a second gRPC e2e suite needs it too).

use sqlx::PgPool;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

/// Starts Postgres and returns (container guard, URL, migrated pool). Keep
/// the guard alive for the test's duration or the container stops.
pub async fn pg() -> (ContainerAsync<Postgres>, String, PgPool) {
    let (container, url) = postgres_url().await;
    let pool_config = koine_store_postgres::PoolConfig::default();
    let pool = koine_store_postgres::connect_pool(&url, pool_config)
        .await
        .expect("connect + migrate");
    (container, url, pool)
}

/// Starts Postgres and returns (container guard, connection URL). Keep the
/// guard alive for the test's duration or the container stops.
pub async fn postgres_url() -> (ContainerAsync<Postgres>, String) {
    let container = Postgres::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    (container, url)
}

/// Production `Clock` implementation (copied from
/// `koine-server/src/runtime.rs`, which is a bin-only crate and so cannot be
/// a test dependency here — see the phase-2B dedup note above).
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl koine_application::ports::Clock for SystemClock {
    fn now(&self) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc::now()
    }
}

/// Production `IdGenerator` implementation (`UUIDv7`, ADR 0010) — copied
/// from `koine-server/src/runtime.rs` for the same reason as `SystemClock`
/// above.
#[derive(Debug, Clone, Copy)]
pub struct UuidV7Ids;

impl koine_application::ports::IdGenerator for UuidV7Ids {
    fn job_id(&self) -> koine_domain::JobId {
        koine_domain::JobId::new(uuid::Uuid::now_v7())
    }
    fn event_id(&self) -> koine_domain::EventId {
        koine_domain::EventId::new(uuid::Uuid::now_v7())
    }
    fn lease_id(&self) -> koine_domain::LeaseId {
        koine_domain::LeaseId::new(uuid::Uuid::now_v7())
    }
    fn correlation_id(&self) -> koine_domain::CorrelationId {
        koine_domain::CorrelationId::new(uuid::Uuid::now_v7())
    }
    fn jitter_seed(&self) -> u64 {
        // High-entropy per the port contract: fold both UUID halves.
        let bits = uuid::Uuid::now_v7().as_u128();
        #[allow(clippy::cast_possible_truncation)] // intentional fold of both halves
        {
            (bits as u64) ^ ((bits >> 64) as u64)
        }
    }
}
