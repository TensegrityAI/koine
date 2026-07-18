//! Ring-3 contract tests for the Postgres event store.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

#[tokio::test]
async fn migrations_apply_cleanly() {
    let (_guard, pool) = support::pg().await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM event_store.events")
        .fetch_one(&pool)
        .await
        .expect("query");
    assert_eq!(n, 0);
}
