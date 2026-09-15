//! End-to-end tests over a real gRPC connection.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::Code;
use tonic::transport::Channel;

use agenttx::config::EngineConfig;
use agenttx::engine::{Engine, ToolRegistry, builtin_tools};
use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
use agenttx::proto::agent_tx_service_client::AgentTxServiceClient;
use agenttx::proto::{
    BeginTransactionRequest, CommitTransactionRequest, ExecuteStepRequest, ExecuteStepResponse,
    GetTransactionRequest, RollbackPolicy, RollbackStrategy, RollbackTransactionRequest,
    StepStatus,
};
use agenttx::storage::{RocksStore, StoreOptions};

struct TestServer {
    client: AgentTxServiceClient<Channel>,
    engine: Arc<Engine>,
    addr: SocketAddr,
    outbox: std::path::PathBuf,
    sandbox: tempfile::TempDir,
    shutdown: Option<oneshot::Sender<()>>,
    server: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
    _db: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let db = tempfile::tempdir().unwrap();
        let sandbox = tempfile::tempdir().unwrap();
        let outbox = db.path().join("outbox.jsonl");

        let store =
            Arc::new(RocksStore::open(db.path().join("rocks"), StoreOptions::default()).unwrap());
        let tools = ToolRegistry::new();
        builtin_tools::register_builtin_tools(&tools, sandbox.path());
        let undo = UndoRegistry::new();
        builtin_tools::register_builtin_undo_factories(&undo);
        let engine = Arc::new(Engine::new(
            store,
            tools,
            undo,
            Arc::new(OutboxDispatcher::new(&outbox)),
            EngineConfig::default(),
        ));

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        let server = tokio::spawn(agenttx::grpc::serve(Arc::clone(&engine), listener, async {
            let _ = rx.await;
        }));
        let client = AgentTxServiceClient::connect(format!("http://{addr}"))
            .await
            .unwrap();

        Self {
            client,
            engine,
            addr,
            outbox,
            sandbox,
            shutdown: Some(tx),
            server: Some(server),
            _db: db,
        }
    }

    async fn begin(&mut self) -> String {
        self.client
            .begin_transaction(BeginTransactionRequest {
                agent_id: "test-agent".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_inner()
            .transaction_id
    }

    async fn step(
        &mut self,
        tx: &str,
        step_id: u32,
        tool: &str,
        args: Value,
    ) -> ExecuteStepResponse {
        self.client
            .execute_step(ExecuteStepRequest {
                transaction_id: tx.into(),
                step_id,
                tool_name: tool.into(),
                arguments_json: args.to_string(),
                raw_context: format!("reasoning for step {step_id}"),
            })
            .await
            .unwrap()
            .into_inner()
    }

    async fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(server) = self.server.take() {
            tokio::time::timeout(Duration::from_secs(5), server)
                .await
                .expect("server shuts down")
                .unwrap()
                .unwrap();
        }
    }
}

#[tokio::test]
async fn begin_execute_commit_publishes_state_and_dispatches_effects() {
    let mut srv = TestServer::start().await;
    let tx = srv.begin().await;

    let put = srv
        .step(
            &tx,
            1,
            "kv.put",
            json!({ "key": "greeting", "value": "hello" }),
        )
        .await;
    assert_eq!(put.status(), StepStatus::Success);
    assert_eq!(put.next_step_id, 2);
    assert_eq!(
        serde_json::from_str::<Value>(&put.output_json).unwrap()["value"],
        "hello"
    );

    let staged = srv
        .step(
            &tx,
            2,
            "effect.stage",
            json!({ "channel": "email", "payload": { "to": "ops@example.com" } }),
        )
        .await;
    assert_eq!(staged.status(), StepStatus::Success);
    assert!(
        !srv.outbox.exists(),
        "effects must not be dispatched before commit"
    );

    // Uncommitted state is invisible outside the transaction.
    assert!(
        srv.engine
            .store()
            .get_committed("kv/greeting")
            .unwrap()
            .is_none()
    );

    let view = srv
        .client
        .get_transaction(GetTransactionRequest {
            transaction_id: tx.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(view.status, "ACTIVE");
    assert_eq!(view.next_step_id, 3);
    assert_eq!(view.context.len(), 2);
    assert_eq!(view.context[1].tool_name, "effect.stage");

    let commit = srv
        .client
        .commit_transaction(CommitTransactionRequest {
            transaction_id: tx.clone(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(commit.committed);
    assert_eq!(commit.steps_committed, 2);
    assert_eq!(commit.effects_dispatched, 1);
    assert!(commit.dispatch_errors.is_empty());

    let committed = srv
        .engine
        .store()
        .get_committed("kv/greeting")
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&committed).unwrap(),
        json!("hello")
    );
    let outbox = std::fs::read_to_string(&srv.outbox).unwrap();
    assert_eq!(outbox.lines().count(), 1);
    assert!(outbox.contains("ops@example.com"));

    let gone = srv
        .client
        .get_transaction(GetTransactionRequest { transaction_id: tx })
        .await
        .unwrap_err();
    assert_eq!(gone.code(), Code::NotFound);
    srv.stop().await;
}

#[tokio::test]
async fn foreign_key_failure_triggers_dependency_jump() {
    let mut srv = TestServer::start().await;
    let tx = srv.begin().await;

    // Step 1: the agent picks a user id that does not exist.
    srv.step(
        &tx,
        1,
        "kv.put",
        json!({ "key": "selected_user", "value": "u-404" }),
    )
    .await;
    srv.step(
        &tx,
        2,
        "fs.write",
        json!({ "path": "draft.txt", "contents": "order draft" }),
    )
    .await;
    assert!(srv.sandbox.path().join("draft.txt").exists());

    // Step 3 consumes step 1's output and violates the foreign key.
    let failed = srv
        .step(
            &tx,
            3,
            "record.insert",
            json!({
                "table": "orders",
                "id": "o-1",
                "fields": { "user_id": "${steps.1.value}" },
                "references": { "user_id": "users" }
            }),
        )
        .await;

    assert_eq!(failed.status(), StepStatus::RollbackTriggered);
    assert_eq!(failed.strategy(), RollbackStrategy::DependencyJump);
    assert_eq!(failed.rollback_to_step, 1);
    assert_eq!(failed.next_step_id, 1);
    assert_eq!(
        failed.clean_hint,
        "Hint: Foreign key constraint failed for 'user_id'. Ensure target user exists before step execution."
    );
    assert_eq!(failed.error_category, "foreign_key_violation");
    assert_eq!(failed.constraints, vec![failed.clean_hint.clone()]);
    assert!(!failed.context_reset);

    // Steps 1-3 were rewound: state and the reversible file write are undone.
    assert!(
        srv.engine
            .store()
            .get(&tx, "kv/selected_user")
            .unwrap()
            .is_none()
    );
    assert!(!srv.sandbox.path().join("draft.txt").exists());

    // The agent follows the hint and retries from step 1.
    srv.step(
        &tx,
        1,
        "record.insert",
        json!({ "table": "users", "id": "u-1" }),
    )
    .await;
    let ok = srv
        .step(
            &tx,
            2,
            "record.insert",
            json!({
                "table": "orders",
                "id": "o-1",
                "fields": { "user_id": "${steps.1.id}" },
                "references": { "user_id": "users" }
            }),
        )
        .await;
    assert_eq!(ok.status(), StepStatus::Success, "{}", ok.clean_hint);
    srv.client
        .commit_transaction(CommitTransactionRequest { transaction_id: tx })
        .await
        .unwrap();
    assert!(
        srv.engine
            .store()
            .get_committed("records/orders/o-1")
            .unwrap()
            .is_some()
    );
    srv.stop().await;
}

#[tokio::test]
async fn protocol_violations_map_to_grpc_status_codes() {
    let mut srv = TestServer::start().await;
    let tx = srv.begin().await;

    let out_of_order = srv
        .client
        .execute_step(ExecuteStepRequest {
            transaction_id: tx.clone(),
            step_id: 3,
            tool_name: "kv.put".into(),
            arguments_json: "{}".into(),
            raw_context: String::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(out_of_order.code(), Code::FailedPrecondition);
    assert!(out_of_order.message().contains("expected step 1"));

    let zero = srv
        .client
        .execute_step(ExecuteStepRequest {
            transaction_id: tx.clone(),
            step_id: 0,
            tool_name: "kv.put".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(zero.code(), Code::InvalidArgument);

    let unknown = srv
        .client
        .commit_transaction(CommitTransactionRequest {
            transaction_id: "nope".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(unknown.code(), Code::NotFound);

    let bad_policy = srv
        .client
        .begin_transaction(BeginTransactionRequest {
            policy: Some(RollbackPolicy {
                max_local_depth: Some(1_000),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(bad_policy.code(), Code::InvalidArgument);

    // An agent-attributable error is *not* an RPC error: it returns a hint.
    let hallucinated = srv.step(&tx, 1, "sql.run", json!({})).await;
    assert_eq!(hallucinated.status(), StepStatus::RollbackTriggered);
    assert!(
        hallucinated
            .clean_hint
            .starts_with("Hint: Tool 'sql.run' does not exist. Use one of:")
    );
    srv.stop().await;
}

#[tokio::test]
async fn manual_rollback_and_full_abort() {
    let mut srv = TestServer::start().await;
    let tx = srv.begin().await;
    for step in 1..=3 {
        let r = srv
            .step(
                &tx,
                step,
                "kv.put",
                json!({ "key": format!("k{step}"), "value": step }),
            )
            .await;
        assert_eq!(r.status(), StepStatus::Success);
    }

    let partial = srv
        .client
        .rollback_transaction(RollbackTransactionRequest {
            transaction_id: tx.clone(),
            to_step: Some(2),
            reason: "agent changed plan".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(partial.next_step_id, 2);
    let store = srv.engine.store();
    assert!(store.get(&tx, "kv/k1").unwrap().is_some());
    assert!(store.get(&tx, "kv/k2").unwrap().is_none());
    assert!(store.get(&tx, "kv/k3").unwrap().is_none());

    let invalid = srv
        .client
        .rollback_transaction(RollbackTransactionRequest {
            transaction_id: tx.clone(),
            to_step: Some(5),
            reason: String::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(invalid.code(), Code::InvalidArgument);

    let abort = srv
        .client
        .rollback_transaction(RollbackTransactionRequest {
            transaction_id: tx.clone(),
            to_step: None,
            reason: "user cancelled".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert!(abort.rolled_back);
    assert_eq!(abort.next_step_id, 0);
    assert!(store.open_transactions().unwrap().is_empty());
    assert!(store.get_committed("kv/k1").unwrap().is_none());
    srv.stop().await;
}

#[tokio::test]
async fn write_conflict_aborts_second_committer() {
    let mut srv = TestServer::start().await;
    let a = srv.begin().await;
    let b = srv.begin().await;
    srv.step(&a, 1, "kv.put", json!({ "key": "inventory", "value": 9 }))
        .await;
    srv.step(&b, 1, "kv.put", json!({ "key": "inventory", "value": 7 }))
        .await;

    srv.client
        .commit_transaction(CommitTransactionRequest { transaction_id: a })
        .await
        .unwrap();
    let conflict = srv
        .client
        .commit_transaction(CommitTransactionRequest {
            transaction_id: b.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(conflict.code(), Code::Aborted);
    assert!(conflict.message().contains("inventory"));

    let value = srv
        .engine
        .store()
        .get_committed("kv/inventory")
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&value).unwrap(), json!(9));
    let gone = srv
        .client
        .get_transaction(GetTransactionRequest { transaction_id: b })
        .await
        .unwrap_err();
    assert_eq!(gone.code(), Code::NotFound);
    srv.stop().await;
}

#[tokio::test]
async fn health_service_reports_serving() {
    use tonic_health::pb::HealthCheckRequest;
    use tonic_health::pb::health_client::HealthClient;

    let srv = TestServer::start().await;
    let channel = Channel::from_shared(format!("http://{}", srv.addr))
        .unwrap()
        .connect()
        .await
        .unwrap();
    let mut health = HealthClient::new(channel);
    let response = health
        .check(HealthCheckRequest {
            service: "agenttx.v1.AgentTxService".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.status(),
        tonic_health::pb::health_check_response::ServingStatus::Serving
    );
    srv.stop().await;
}
