//! Compiles `proto/agenttx/v1/agenttx.proto` into tonic/prost Rust code.
//!
//! Uses `protox` (a pure-Rust protobuf compiler) so building the crate does
//! not require a system `protoc` installation.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto = "proto/agenttx/v1/agenttx.proto";
    println!("cargo:rerun-if-changed={proto}");

    let descriptors = protox::compile([proto], ["proto"])?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_fds(descriptors)?;
    Ok(())
}
