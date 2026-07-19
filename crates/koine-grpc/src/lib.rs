//! Koiné data plane driving adapter: worker fetch stream, ack/fail, heartbeats over `gRPC`.

pub mod auth;
pub mod service;

pub use service::{Deps, GrpcConfig, WorkerApi, server};
