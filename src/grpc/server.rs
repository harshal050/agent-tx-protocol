//! gRPC server bootstrap with health checking and graceful shutdown.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;

use super::proto::agent_tx_service_server::AgentTxServiceServer;
use super::service::AgentTxGrpcService;
use crate::engine::Engine;

/// Maximum decoded request size (arguments + context + framing).
pub const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;

/// Serves `AgentTxService` and `grpc.health.v1.Health` on `listener` until
/// `shutdown` resolves, then drains in-flight requests.
pub async fn serve<F>(engine: Arc<Engine>, listener: TcpListener, shutdown: F) -> anyhow::Result<()>
where
    F: Future<Output = ()> + Send,
{
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<AgentTxServiceServer<AgentTxGrpcService>>()
        .await;

    let service = AgentTxServiceServer::new(AgentTxGrpcService::new(engine))
        .max_decoding_message_size(MAX_MESSAGE_BYTES);

    Server::builder()
        .tcp_nodelay(true)
        .http2_keepalive_interval(Some(Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(Duration::from_secs(10)))
        .add_service(health_service)
        .add_service(service)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown)
        .await?;
    Ok(())
}
