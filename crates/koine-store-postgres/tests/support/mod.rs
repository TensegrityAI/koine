//! Ring-3 harness: one throwaway Postgres container per test, real migrations.

use sqlx::PgPool;
use testcontainers::ContainerAsync;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;

/// Starts Postgres and returns (container guard, migrated pool). Keep the
/// guard alive for the test's duration or the container stops.
pub async fn pg() -> (ContainerAsync<Postgres>, PgPool) {
    let container = Postgres::default().start().await.expect("start postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("mapped port");
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = koine_store_postgres::connect_pool(&url)
        .await
        .expect("connect + migrate");
    (container, pool)
}
