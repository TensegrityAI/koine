//! `serve`: the long-running authenticated `gRPC` data-plane server (ADR
//! 0014). Wires the Postgres adapters built in phase 1B to the worker
//! surface built in phase 2A, plus the background sweep and outbox-relay
//! tickers that keep leases and dispatch honest without a separate process.

use std::net::SocketAddr;
use std::num::{NonZeroU32, NonZeroU64};
use std::sync::Arc;
use std::time::Duration;

use koine_application::use_cases::sweep::SweepExpiredLeases;
use koine_grpc::{Deps, GrpcConfig, server};
use koine_store_postgres::{
    PgPresence, PgSignal, PoolConfig, PostgresDispatcher, PostgresEventStore, PostgresOutboxRelay,
    connect_pool,
};

use crate::runtime::{SystemClock, UuidV7Ids};
use crate::sinks::PrintingSink;

/// Default `DATABASE_URL`, matching `dev-loop`'s default.
const DEFAULT_DATABASE_URL: &str = "postgres://koine:koine@localhost:5432/koine";
/// Default `gRPC` bind address.
const DEFAULT_GRPC_ADDR: &str = "0.0.0.0:7419";
/// Default lease-ttl ceiling every worker's requested `lease_ttl_ms` is
/// clamped to: 5 minutes.
const DEFAULT_MAX_LEASE_TTL_MS: u64 = 300_000;
/// Default idle-poll fallback for a drained `Fetch` stream: 1 second.
const DEFAULT_IDLE_POLL_MS: u64 = 1_000;
/// Default maximum number of connections in the shared operational pool.
const DEFAULT_DB_MAX_CONNECTIONS: u32 = 16;
/// Default maximum wait for an operational-pool connection: 5 seconds.
const DEFAULT_DB_ACQUIRE_TIMEOUT_MS: u64 = 5_000;
/// Sweep and outbox-relay ticker period.
const TICKER_PERIOD: Duration = Duration::from_millis(500);
/// Outbox-relay batch size per tick.
const RELAY_BATCH: i64 = 64;

/// `serve`'s configuration, sourced from environment variables by
/// [`parse_config`].
#[derive(Debug)]
struct ServeConfig {
    /// Postgres connection string.
    database_url: String,
    /// The bearer token every worker must present (ADR 0014).
    worker_token: String,
    /// Address the `gRPC` server binds to.
    grpc_addr: SocketAddr,
    /// Ceiling every requested lease TTL is clamped to.
    max_lease_ttl: Duration,
    /// How long a drained `Fetch` poll waits for a dispatch signal.
    idle_poll: Duration,
    /// Bounded settings for the shared operational Postgres pool.
    pool_config: PoolConfig,
}

/// Reads `serve`'s configuration through `lookup` (`std::env::var(name).ok()`
/// in production; an in-memory map in tests).
///
/// # Errors
///
/// Returns an error string if `KOINE_WORKER_TOKEN` is missing or empty —
/// the data plane must never start unauthenticated (ADR 0014). Treating an
/// empty token exactly like a missing one matters in practice: shell
/// interpolation of an unset variable (`KOINE_WORKER_TOKEN=${UNSET_VAR}`)
/// yields an empty string, and starting anyway would launch a server whose
/// auth interceptor is silently misconfigured. Also errors if
/// `KOINE_GRPC_ADDR` or any numeric resource setting is zero or malformed.
fn parse_config(lookup: impl Fn(&str) -> Option<String>) -> Result<ServeConfig, String> {
    let worker_token = lookup("KOINE_WORKER_TOKEN").filter(|token| !token.is_empty());
    let Some(worker_token) = worker_token else {
        return Err("KOINE_WORKER_TOKEN is required".to_string());
    };
    let database_url = lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_string());
    let addr_raw = lookup("KOINE_GRPC_ADDR").unwrap_or_else(|| DEFAULT_GRPC_ADDR.to_string());
    let grpc_addr = addr_raw
        .parse::<SocketAddr>()
        .map_err(|e| format!("KOINE_GRPC_ADDR {addr_raw:?}: {e}"))?;
    let max_lease_ttl_ms =
        parse_non_zero_u64(&lookup, "KOINE_MAX_LEASE_TTL_MS", DEFAULT_MAX_LEASE_TTL_MS)?;
    let idle_poll_ms = parse_non_zero_u64(&lookup, "KOINE_IDLE_POLL_MS", DEFAULT_IDLE_POLL_MS)?;
    let max_connections = parse_non_zero_u32(
        &lookup,
        "KOINE_DB_MAX_CONNECTIONS",
        DEFAULT_DB_MAX_CONNECTIONS,
    )?;
    let acquire_timeout_ms = parse_non_zero_u64(
        &lookup,
        "KOINE_DB_ACQUIRE_TIMEOUT_MS",
        DEFAULT_DB_ACQUIRE_TIMEOUT_MS,
    )?;

    Ok(ServeConfig {
        database_url,
        worker_token,
        grpc_addr,
        max_lease_ttl: Duration::from_millis(max_lease_ttl_ms.get()),
        idle_poll: Duration::from_millis(idle_poll_ms.get()),
        pool_config: PoolConfig::new(max_connections, acquire_timeout_ms),
    })
}

fn parse_non_zero_u64(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &str,
    default: u64,
) -> Result<NonZeroU64, String> {
    let value = match lookup(name) {
        None => default,
        Some(raw) => raw
            .parse::<u64>()
            .map_err(|e| format!("{name} {raw:?}: {e}"))?,
    };
    NonZeroU64::new(value).ok_or_else(|| format!("{name} must be greater than zero"))
}

fn parse_non_zero_u32(
    lookup: &impl Fn(&str) -> Option<String>,
    name: &str,
    default: u32,
) -> Result<NonZeroU32, String> {
    let value = match lookup(name) {
        None => default,
        Some(raw) => raw
            .parse::<u32>()
            .map_err(|e| format!("{name} {raw:?}: {e}"))?,
    };
    NonZeroU32::new(value).ok_or_else(|| format!("{name} must be greater than zero"))
}

/// Runs the authenticated `gRPC` data-plane server: connects and migrates,
/// wires the Postgres adapters, spawns the sweep and outbox-relay tickers,
/// then serves `WorkerService` until `Ctrl-C` resolves.
///
/// # Errors
///
/// Returns an error string if the environment is misconfigured (see
/// [`parse_config`]), the database connection/migration or initial listener
/// setup fails, or the `gRPC` transport fails to bind or serve.
pub async fn run() -> Result<(), String> {
    let cfg = parse_config(|name| std::env::var(name).ok())?;

    let pool = connect_pool(&cfg.database_url, cfg.pool_config)
        .await
        .map_err(|e| format!("connect/migrate: {e}"))?;
    let signal = PgSignal::connect(
        &cfg.database_url,
        pool.clone(),
        cfg.pool_config.acquire_timeout(),
    )
    .await
    .map_err(|e| format!("listen koine_dispatch: {e}"))?;

    // Sweep ticker: reclaims expired leases every 500ms so a crashed worker's
    // job becomes claimable again without a separate process (ADR 0008).
    let sweep_pool = pool.clone();
    tokio::spawn(async move {
        let dispatcher =
            PostgresDispatcher::new(sweep_pool, Arc::new(UuidV7Ids), Arc::new(SystemClock));
        let sweep = SweepExpiredLeases {
            dispatcher: &dispatcher,
        };
        let mut ticker = tokio::time::interval(TICKER_PERIOD);
        loop {
            ticker.tick().await;
            match sweep.execute().await {
                Ok(0) => {}
                Ok(swept) => println!("serve: sweep recovered {swept} expired lease(s)"),
                Err(e) => eprintln!("serve: sweep error: {e}"),
            }
        }
    });

    // Outbox-relay ticker: drains the transactional outbox every 500ms.
    let relay_pool = pool.clone();
    tokio::spawn(async move {
        let relay = PostgresOutboxRelay::new(relay_pool);
        let sink = PrintingSink;
        let mut ticker = tokio::time::interval(TICKER_PERIOD);
        loop {
            ticker.tick().await;
            if let Err(e) = relay.relay_once(&sink, RELAY_BATCH).await {
                eprintln!("serve: relay error: {e}");
            }
        }
    });

    let store = PostgresEventStore::new(pool.clone());
    let dispatcher =
        PostgresDispatcher::new(pool.clone(), Arc::new(UuidV7Ids), Arc::new(SystemClock));
    let presence = PgPresence::new(pool);
    let grpc_addr = cfg.grpc_addr;
    let deps = Arc::new(Deps {
        store,
        dispatcher,
        ids: UuidV7Ids,
        clock: SystemClock,
        signal,
        presence,
        config: GrpcConfig {
            token: cfg.worker_token,
            max_lease_ttl: cfg.max_lease_ttl,
            idle_poll: cfg.idle_poll,
        },
    });

    println!(
        "koine-server: serve listening on {grpc_addr} — authenticated grpc data plane (all queues)"
    );

    tonic::transport::Server::builder()
        .add_service(server(deps))
        .serve_with_shutdown(grpc_addr, async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| format!("grpc serve: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// Builds a `lookup` closure over a fixed map, mirroring how
    /// `run` closes over `std::env::var` in production.
    fn lookup<'a>(vars: &'a HashMap<&'a str, &'a str>) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.get(name).map(|v| (*v).to_string())
    }

    #[test]
    fn missing_token_is_refused() {
        let vars = HashMap::new();
        let err = parse_config(lookup(&vars)).expect_err("missing token must refuse");
        assert_eq!(err, "KOINE_WORKER_TOKEN is required");
    }

    #[test]
    fn empty_token_is_refused() {
        // The controller-authorized addition: an explicitly-empty token
        // (e.g. `KOINE_WORKER_TOKEN=${UNSET_VAR}` interpolation) refuses
        // exactly like a missing one.
        let vars = HashMap::from([("KOINE_WORKER_TOKEN", "")]);
        let err = parse_config(lookup(&vars)).expect_err("empty token must refuse");
        assert_eq!(err, "KOINE_WORKER_TOKEN is required");
    }

    #[test]
    fn defaults_apply_when_only_token_is_set() {
        let vars = HashMap::from([("KOINE_WORKER_TOKEN", "t")]);
        let cfg = parse_config(lookup(&vars)).expect("valid token");
        assert_eq!(cfg.database_url, DEFAULT_DATABASE_URL);
        assert_eq!(
            cfg.grpc_addr,
            DEFAULT_GRPC_ADDR
                .parse::<SocketAddr>()
                .expect("valid default addr")
        );
        assert_eq!(
            cfg.max_lease_ttl,
            Duration::from_millis(DEFAULT_MAX_LEASE_TTL_MS)
        );
        assert_eq!(cfg.idle_poll, Duration::from_millis(DEFAULT_IDLE_POLL_MS));
        assert_eq!(cfg.pool_config.max_connections().get(), 16);
        assert_eq!(cfg.pool_config.acquire_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn overrides_are_parsed() {
        let vars = HashMap::from([
            ("KOINE_WORKER_TOKEN", "t"),
            ("DATABASE_URL", "postgres://x:y@host/db"),
            ("KOINE_GRPC_ADDR", "127.0.0.1:9000"),
            ("KOINE_MAX_LEASE_TTL_MS", "60000"),
            ("KOINE_IDLE_POLL_MS", "250"),
            ("KOINE_DB_MAX_CONNECTIONS", "4"),
            ("KOINE_DB_ACQUIRE_TIMEOUT_MS", "900"),
        ]);
        let cfg = parse_config(lookup(&vars)).expect("valid overrides");
        assert_eq!(cfg.worker_token, "t");
        assert_eq!(cfg.database_url, "postgres://x:y@host/db");
        assert_eq!(
            cfg.grpc_addr,
            "127.0.0.1:9000".parse::<SocketAddr>().expect("valid addr")
        );
        assert_eq!(cfg.max_lease_ttl, Duration::from_mins(1));
        assert_eq!(cfg.idle_poll, Duration::from_millis(250));
        assert_eq!(cfg.pool_config.max_connections().get(), 4);
        assert_eq!(
            cfg.pool_config.acquire_timeout(),
            Duration::from_millis(900)
        );
    }

    #[test]
    fn invalid_addr_is_rejected() {
        let vars = HashMap::from([
            ("KOINE_WORKER_TOKEN", "t"),
            ("KOINE_GRPC_ADDR", "not-an-addr"),
        ]);
        let err = parse_config(lookup(&vars)).expect_err("invalid addr must error");
        assert!(err.contains("KOINE_GRPC_ADDR"));
    }

    #[test]
    fn invalid_ttl_is_rejected() {
        let vars = HashMap::from([
            ("KOINE_WORKER_TOKEN", "t"),
            ("KOINE_MAX_LEASE_TTL_MS", "not-a-number"),
        ]);
        let err = parse_config(lookup(&vars)).expect_err("invalid ttl must error");
        assert!(err.contains("KOINE_MAX_LEASE_TTL_MS"));
    }

    #[test]
    fn invalid_idle_poll_is_rejected() {
        let vars = HashMap::from([
            ("KOINE_WORKER_TOKEN", "t"),
            ("KOINE_IDLE_POLL_MS", "not-a-number"),
        ]);
        let err = parse_config(lookup(&vars)).expect_err("invalid idle poll must error");
        assert!(err.contains("KOINE_IDLE_POLL_MS"));
    }

    #[test]
    fn zero_resource_values_are_rejected() {
        for name in [
            "KOINE_MAX_LEASE_TTL_MS",
            "KOINE_IDLE_POLL_MS",
            "KOINE_DB_MAX_CONNECTIONS",
            "KOINE_DB_ACQUIRE_TIMEOUT_MS",
        ] {
            let vars = HashMap::from([("KOINE_WORKER_TOKEN", "t"), (name, "0")]);
            let err = parse_config(lookup(&vars)).expect_err("zero must fail");
            assert!(err.contains(name));
        }
    }

    #[test]
    fn invalid_pool_size_is_rejected() {
        let vars = HashMap::from([
            ("KOINE_WORKER_TOKEN", "t"),
            ("KOINE_DB_MAX_CONNECTIONS", "not-a-number"),
        ]);
        let err = parse_config(lookup(&vars)).expect_err("invalid pool size must error");
        assert!(err.contains("KOINE_DB_MAX_CONNECTIONS"));
    }

    #[test]
    fn invalid_acquire_timeout_is_rejected() {
        let vars = HashMap::from([
            ("KOINE_WORKER_TOKEN", "t"),
            ("KOINE_DB_ACQUIRE_TIMEOUT_MS", "not-a-number"),
        ]);
        let err = parse_config(lookup(&vars)).expect_err("invalid acquire timeout must error");
        assert!(err.contains("KOINE_DB_ACQUIRE_TIMEOUT_MS"));
    }
}
