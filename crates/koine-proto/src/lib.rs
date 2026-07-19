//! Koiné wire contract: versioned protobuf definitions and generated gRPC types.

/// The koine.v1 data-plane contract (generated; see ADR 0013).
#[allow(missing_docs, clippy::pedantic, clippy::nursery)] // generated code
pub mod v1 {
    tonic::include_proto!("koine.v1");
}
