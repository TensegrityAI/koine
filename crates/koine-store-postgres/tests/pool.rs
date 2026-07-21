//! Verifies that explicit pool limits reach `sqlx` unchanged.
// clippy.toml's allow-expect-in-tests only covers #[test] fns, not shared helpers.
#![allow(clippy::expect_used)]

mod support;

use std::num::{NonZeroU32, NonZeroU64};
use std::time::Duration;

use koine_store_postgres::{PoolConfig, connect_pool};

#[tokio::test]
async fn pool_options_are_honored() {
    let (_guard, url) = support::postgres_url().await;
    let config = PoolConfig::new(
        NonZeroU32::new(2).expect("non-zero"),
        NonZeroU64::new(750).expect("non-zero"),
    );
    let pool = connect_pool(&url, config).await.expect("connect");
    assert_eq!(pool.options().get_max_connections(), 2);
    assert_eq!(
        pool.options().get_acquire_timeout(),
        Duration::from_millis(750)
    );
}
