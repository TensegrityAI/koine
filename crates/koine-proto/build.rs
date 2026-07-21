//! Generates the koine.v1 gRPC contract with the pinned vendored compiler.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    let mut prost = tonic_prost_build::Config::new();
    prost.protoc_executable(protoc);
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_with_config(prost, &["proto/koine/v1/worker.proto"], &["proto"])?;
    Ok(())
}
