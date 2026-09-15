//! gRPC transport (tonic).

pub mod server;
pub mod service;

/// Generated protobuf types and client/server stubs for `agenttx.v1`.
pub mod proto {
    #![allow(clippy::all, missing_docs)]
    tonic::include_proto!("agenttx.v1");
}

pub use server::serve;
pub use service::AgentTxGrpcService;
