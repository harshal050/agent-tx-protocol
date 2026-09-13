use axum::{
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

#[derive(Deserialize, Serialize, Debug)]
struct AgentRequest {
    agent_id: String,
    prompt: String,
}

#[derive(Serialize)]
struct AgentResponse {
    status: String,
    message: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/v1/intercept", post(intercept_transaction));

    let addr: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 8080));
    println!("🚀 AgentTx Core Engine running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    return "helth ok"
}

async fn intercept_transaction(
    Json(payload): Json<AgentRequest>,
) -> Json<AgentResponse> {
    println!("📥 [Tx Intercepted] From Agent: {}", payload.agent_id);
    println!("📝 Prompt Payload: {}", payload.prompt);

    Json(AgentResponse {
        status: "SUCCESS".to_string(),
        message: "Transaction logged by AgentTx Kernel".to_string(),
    })
}



