//! Generates the koine.v1 gRPC contract at build time.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/koine/v1/worker.proto"], &["proto"])?;
    Ok(())
}
