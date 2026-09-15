//! Engine-level simulations of agent failure patterns and rollback strategies.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::{Value, json};

use agenttx::AgentTxError;
use agenttx::config::EngineConfig;
use agenttx::engine::{
    Engine, RollbackPolicy, RollbackStrategy, StepOutcome, StepRequest, StepStatus, Tool,
    ToolContext, ToolError, ToolRegistry, builtin_tools,
};
use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
use agenttx::storage::{RocksStore, SnapshotId, StoreOptions};

/// Always fails with `args.error`, counting invocations.
struct AlwaysFail(Arc<AtomicU32>);

#[async_trait]
impl Tool for AlwaysFail {
    fn name(&self) -> &str {
        "sim.fail"
    }

    async fn execute(&self, _ctx: &ToolContext, args: Value) -> Result<Value, ToolError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(ToolError::failed(args["error"].as_str().unwrap_or("boom").to_owned()))
    }
}

struct Harness {
    engine: Arc<Engine>,
    sandbox: tempfile::TempDir,
    db: tempfile::TempDir,
    fail_calls: Arc<AtomicU32>,
}

fn build_engine(db: &std::path::Path, sandbox: &std::path::Path, config: EngineConfig, fail_calls: &Arc<AtomicU32>) -> Arc<Engine> {
    let store = Arc::new(RocksStore::open(db.join("rocks"), StoreOptions::default()).unwrap());
    let tools = ToolRegistry::new();
    builtin_tools::register_builtin_tools(&tools, sandbox);
    tools.register(Arc::new(AlwaysFail(Arc::clone(fail_calls))));
    let undo = UndoRegistry::new();
    builtin_tools::register_builtin_undo_factories(&undo);
    Arc::new(Engine::new(
        store,
        tools,
        undo,
        Arc::new(OutboxDispatcher::new(db.join("outbox.jsonl"))),
        config,
    ))
}

fn harness(config: EngineConfig) -> Harness {
    let db = tempfile::tempdir().unwrap();
    let sandbox = tempfile::tempdir().unwrap();
    let fail_calls = Arc::new(AtomicU32::new(0));
    let engine = build_engine(db.path(), sandbox.path(), config, &fail_calls);
    Harness { engine, sandbox, db, fail_calls }
}

fn policy(max_local_depth: u32, max_global_resets: u32, replay_budget_factor: f64) -> EngineConfig {
    EngineConfig {
        default_policy: RollbackPolicy {
            max_local_depth,
            max_global_resets,
            replay_budget_factor,
        },
        ..EngineConfig::default()
    }
}

async fn begin(engine: &Engine) -> String {
    engine.begin("sim-agent", HashMap::new(), None).await.unwrap().tx_id
}

async fn step(engine: &Engine, tx: &str, step_id: u32, tool: &str, args: Value) -> StepOutcome {
    engine
        .execute_step(StepRequest {
            tx_id: tx.into(),
            step_id,
            tool_name: tool.into(),
            arguments_json: args.to_string(),
            raw_context: format!("thought for step {step_id}"),
        })
        .await
        .unwrap()
}

/// A stubborn agent: replays a fixed plan from whatever step the proxy
/// requests until the transaction ends. Returns the rollback outcomes seen and
/// the total number of step executions.
async fn drive(engine: &Engine, tx: &str, plan: &[(&str, Value)]) -> (Vec<StepOutcome>, u32) {
    let mut next = 1u32;
    let mut rollbacks = Vec::new();
    let mut executions = 0;
    while next >= 1 && (next as usize) <= plan.len() {
        let (tool, args) = &plan[next as usize - 1];
        let outcome = step(engine, tx, next, tool, args.clone()).await;
        executions += 1;
        assert!(executions < 500, "agent loop failed to terminate");
        next = outcome.next_step;
        if outcome.status != StepStatus::Success {
            rollbacks.push(outcome);
        }
    }
    (rollbacks, executions)
}

#[tokio::test]
async fn local_backtracks_escalate_to_global_reset_then_abort() {
    let h = harness(policy(2, 1, 100.0));
    let tx = begin(&h.engine).await;
    let plan = [
        ("kv.put", json!({ "key": "a", "value": 1 })),
        ("kv.put", json!({ "key": "b", "value": 2 })),
        ("kv.put", json!({ "key": "c", "value": 3 })),
        ("sim.fail", json!({ "error": "RuntimeError: widget exploded" })),
    ];
    let (rollbacks, executions) = drive(&h.engine, &tx, &plan).await;

    let trace: Vec<(RollbackStrategy, u32)> = rollbacks
        .iter()
        .map(|o| (o.strategy, o.rollback_to_step))
        .collect();
    assert_eq!(
        trace,
        vec![
            (RollbackStrategy::LocalBacktrack, 3),
            (RollbackStrategy::LocalBacktrack, 2),
            (RollbackStrategy::GlobalReset, 0),
            (RollbackStrategy::LocalBacktrack, 3),
            (RollbackStrategy::LocalBacktrack, 2),
            (RollbackStrategy::Abort, 0),
        ]
    );
    let reset = &rollbacks[2];
    assert!(reset.context_reset);
    assert_eq!(reset.next_step, 1);
    assert!(reset.constraints.iter().any(|c| c.contains("RuntimeError: widget exploded")));

    let abort = rollbacks.last().unwrap();
    assert_eq!(abort.status, StepStatus::Failed);
    assert!(abort.abort_reason.as_deref().unwrap().contains("global reset budget"));

    // Bounded work: 2 epochs × (4 + 2 + 3) executions.
    assert_eq!(executions, 18);
    assert_eq!(h.fail_calls.load(Ordering::SeqCst), 6);

    // The aborted transaction left no trace.
    assert!(matches!(h.engine.get(&tx).await, Err(AgentTxError::TransactionNotFound(_))));
    let store = h.engine.store();
    assert!(store.open_transactions().unwrap().is_empty());
    assert!(store.get(&tx, "kv/a").unwrap().is_none());
}

#[tokio::test]
async fn dependency_jump_rewinds_state_compensates_saga_and_discards_effects() {
    let h = harness(EngineConfig::default());
    let tx = begin(&h.engine).await;
    let report = h.sandbox.path().join("report.txt");
    std::fs::write(&report, "original").unwrap();

    assert_eq!(step(&h.engine, &tx, 1, "kv.put", json!({ "key": "customer", "value": "c-404" })).await.status, StepStatus::Success);
    assert_eq!(step(&h.engine, &tx, 2, "fs.write", json!({ "path": "report.txt", "contents": "draft" })).await.status, StepStatus::Success);
    assert_eq!(step(&h.engine, &tx, 3, "effect.stage", json!({ "channel": "email", "payload": { "to": "cfo@example.com" } })).await.status, StepStatus::Success);
    assert_eq!(std::fs::read_to_string(&report).unwrap(), "draft");
    assert_eq!(h.engine.get(&tx).await.unwrap().pending_effects, 1);

    let failed = step(
        &h.engine,
        &tx,
        4,
        "record.insert",
        json!({
            "table": "invoices",
            "id": "inv-1",
            "fields": { "customer_id": "${steps.1.value}", "amount": 120 },
            "references": { "customer_id": "customers" }
        }),
    )
    .await;

    assert_eq!(failed.status, StepStatus::RollbackTriggered);
    assert_eq!(failed.strategy, RollbackStrategy::DependencyJump);
    assert_eq!(failed.rollback_to_step, 1);
    assert_eq!(
        failed.hint.as_ref().unwrap().text,
        "Hint: Foreign key constraint failed for 'customer_id'. Ensure target customer exists before step execution."
    );
    assert!(failed.compensation_failures.is_empty());
    assert!(
        failed.rollback_latency < Duration::from_millis(50),
        "rollback took {:?}",
        failed.rollback_latency
    );

    // RocksDB state, external file and staged email are all reverted.
    assert!(h.engine.store().get(&tx, "kv/customer").unwrap().is_none());
    assert_eq!(std::fs::read_to_string(&report).unwrap(), "original");
    let view = h.engine.get(&tx).await.unwrap();
    assert_eq!(view.pending_effects, 0);
    assert_eq!(view.next_step, 1);
    assert!(view.context.is_empty());

    // Corrected plan commits cleanly.
    step(&h.engine, &tx, 1, "record.insert", json!({ "table": "customers", "id": "c-1" })).await;
    step(&h.engine, &tx, 2, "fs.write", json!({ "path": "report.txt", "contents": "final" })).await;
    step(&h.engine, &tx, 3, "effect.stage", json!({ "channel": "email", "payload": { "to": "cfo@example.com" } })).await;
    let ok = step(
        &h.engine,
        &tx,
        4,
        "record.insert",
        json!({
            "table": "invoices",
            "id": "inv-1",
            "fields": { "customer_id": "${steps.1.id}", "amount": 120 },
            "references": { "customer_id": "customers" }
        }),
    )
    .await;
    assert_eq!(ok.status, StepStatus::Success);
    // The constraint learned from the failure is still carried forward.
    assert_eq!(ok.constraints.len(), 1);

    let commit = h.engine.commit(&tx).await.unwrap();
    assert_eq!(commit.steps_committed, 4);
    assert_eq!(commit.effects_dispatched, 1);
    assert_eq!(std::fs::read_to_string(&report).unwrap(), "final");
    assert!(h.engine.store().get_committed("records/invoices/inv-1").unwrap().is_some());
    assert!(h.engine.store().saga_records(&tx).unwrap().is_empty());
}

#[tokio::test]
async fn unresolved_reference_jumps_to_producer() {
    let h = harness(EngineConfig::default());
    let tx = begin(&h.engine).await;
    step(&h.engine, &tx, 1, "kv.put", json!({ "key": "user", "value": { "name": "Ada" } })).await;
    step(&h.engine, &tx, 2, "kv.put", json!({ "key": "note", "value": "unrelated" })).await;
    let failed = step(&h.engine, &tx, 3, "kv.put", json!({ "key": "copy", "value": "${steps.1.user_id}" })).await;
    assert_eq!(failed.strategy, RollbackStrategy::DependencyJump);
    assert_eq!(failed.rollback_to_step, 1);
    assert_eq!(
        failed.hint.unwrap().text,
        "Hint: Step 1 output has no 'user_id'. Re-run step 1 so it returns 'user_id', or fix the reference."
    );
}

#[tokio::test]
async fn global_reset_clears_context_and_keeps_constraints() {
    let h = harness(policy(0, 2, 2.0));
    let tx = begin(&h.engine).await;
    step(&h.engine, &tx, 1, "fs.write", json!({ "path": "out.txt", "contents": "x" })).await;
    step(&h.engine, &tx, 2, "kv.put", json!({ "key": "k", "value": true })).await;
    let reset = step(&h.engine, &tx, 3, "sim.fail", json!({ "error": "HTTP 503 Service Unavailable" })).await;

    assert_eq!(reset.strategy, RollbackStrategy::GlobalReset);
    assert_eq!(reset.rollback_to_step, 0);
    assert_eq!(reset.next_step, 1);
    assert!(reset.context_reset);
    assert!(!h.sandbox.path().join("out.txt").exists());

    let view = h.engine.get(&tx).await.unwrap();
    assert!(view.context.is_empty());
    assert_eq!(view.global_resets, 1);
    assert_eq!(
        view.constraints,
        vec!["Hint: Upstream service failed (HTTP 503). Retry later or use an alternative service.".to_string()]
    );

    // A different failure in the next epoch aggregates a second constraint.
    let again = step(&h.engine, &tx, 1, "sim.fail", json!({ "error": "connection refused" })).await;
    assert_eq!(again.strategy, RollbackStrategy::GlobalReset);
    assert_eq!(again.constraints.len(), 2);
}

#[tokio::test]
async fn replay_budget_prevents_quadratic_backtracking() {
    // N = 8, c = 0.1 → budget = ceil(0.1 · 8 · 4) = 4 replayed steps.
    let h = harness(policy(2, 1, 0.1));
    let tx = begin(&h.engine).await;
    let mut plan: Vec<(&str, Value)> = (1..=7)
        .map(|i| ("kv.put", json!({ "key": format!("k{i}"), "value": i })))
        .collect();
    plan.push(("sim.fail", json!({ "error": "ValueError: bad total" })));

    let (rollbacks, _) = drive(&h.engine, &tx, &plan).await;
    // Depth-1 backtrack costs 2 (within budget); depth-2 would cost 3 more.
    assert_eq!(rollbacks[0].strategy, RollbackStrategy::LocalBacktrack);
    assert_eq!(rollbacks[1].strategy, RollbackStrategy::GlobalReset);
}

#[tokio::test]
async fn snapshot_restore_of_thousands_of_writes_is_sub_50ms() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RocksStore::open(dir.path(), StoreOptions::default()).unwrap());
    let tx = store.scoped("bench").unwrap();
    tx.put("anchor", b"kept").unwrap();
    let snapshot = tx.create_snapshot(1).unwrap();

    let writes = 5_000;
    let payload = vec![b'x'; 256];
    for i in 0..writes {
        tx.put(&format!("key-{}", i % 1_000), &payload).unwrap();
    }
    let started = Instant::now();
    let stats = tx.restore_snapshot(&snapshot).unwrap();
    let wall = started.elapsed();

    assert_eq!(stats.entries_reverted, writes);
    assert!(tx.get("key-1").unwrap().is_none());
    assert_eq!(tx.get("anchor").unwrap().as_deref(), Some(&b"kept"[..]));
    println!("restored {writes} journaled writes in {wall:?}");
    assert!(wall < Duration::from_millis(50), "restore took {wall:?}");
    assert!(store.snapshot_meta(&SnapshotId::new("bench", 1)).unwrap().is_some());
}

#[tokio::test]
async fn crash_recovery_compensates_interrupted_transactions() {
    let h = harness(EngineConfig::default());
    let tx = begin(&h.engine).await;
    step(&h.engine, &tx, 1, "fs.write", json!({ "path": "partial.txt", "contents": "half-done" })).await;
    step(&h.engine, &tx, 2, "kv.put", json!({ "key": "k", "value": 1 })).await;
    let file = h.sandbox.path().join("partial.txt");
    assert!(file.exists());

    // Simulate a crash: drop the engine without committing.
    let Harness { engine, sandbox, db, fail_calls } = h;
    drop(engine);

    let restarted = build_engine(db.path(), sandbox.path(), EngineConfig::default(), &fail_calls);
    let report = restarted.recover().await.unwrap();
    assert_eq!(report.transactions, 1);
    assert_eq!(report.compensations_run, 1);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!file.exists(), "reversible effect must be undone");
    assert!(restarted.store().open_transactions().unwrap().is_empty());
    assert!(restarted.store().get(&tx, "kv/k").unwrap().is_none());
}

#[tokio::test]
async fn concurrent_transactions_are_isolated_and_all_commit() {
    let h = harness(EngineConfig::default());
    let tasks: Vec<_> = (0..16)
        .map(|i| {
            let engine = Arc::clone(&h.engine);
            tokio::spawn(async move {
                let tx = begin(&engine).await;
                for s in 1..=5u32 {
                    let out = step(&engine, &tx, s, "kv.put", json!({ "key": format!("t{i}-s{s}"), "value": i })).await;
                    assert_eq!(out.status, StepStatus::Success);
                }
                // Other transactions' uncommitted writes are invisible.
                let other = format!("t{}-s1", (i + 1) % 16);
                assert!(engine.store().get(&tx, &format!("kv/{other}")).unwrap().is_none()
                    || engine.store().get_committed(&format!("kv/{other}")).unwrap().is_some());
                engine.commit(&tx).await.unwrap()
            })
        })
        .collect();
    for task in tasks {
        assert_eq!(task.await.unwrap().steps_committed, 5);
    }
    for i in 0..16 {
        assert!(h.engine.store().get_committed(&format!("kv/t{i}-s5")).unwrap().is_some());
    }
    assert_eq!(h.engine.active_transactions(), 0);
}

#[tokio::test]
async fn idle_transactions_are_reaped() {
    let h = harness(EngineConfig {
        tx_idle_timeout: Some(Duration::from_millis(40)),
        ..EngineConfig::default()
    });
    let stale = begin(&h.engine).await;
    step(&h.engine, &stale, 1, "fs.write", json!({ "path": "stale.txt", "contents": "x" })).await;
    tokio::time::sleep(Duration::from_millis(60)).await;
    let fresh = begin(&h.engine).await;

    assert_eq!(h.engine.reap_idle().await, 1);
    assert!(matches!(h.engine.get(&stale).await, Err(AgentTxError::TransactionNotFound(_))));
    assert!(h.engine.get(&fresh).await.is_ok());
    assert!(!h.sandbox.path().join("stale.txt").exists());
}

#[tokio::test]
async fn malformed_arguments_and_timeouts_become_hints() {
    let h = harness(EngineConfig {
        step_timeout: Duration::from_millis(30),
        ..EngineConfig::default()
    });
    let tx = begin(&h.engine).await;
    let malformed = h
        .engine
        .execute_step(StepRequest {
            tx_id: tx.clone(),
            step_id: 1,
            tool_name: "kv.put".into(),
            arguments_json: "{key: 'unquoted'}".into(),
            raw_context: String::new(),
        })
        .await
        .unwrap();
    assert_eq!(malformed.status, StepStatus::RollbackTriggered);
    assert_eq!(
        malformed.hint.unwrap().text,
        "Hint: Malformed JSON. Emit a single valid JSON object with double-quoted keys."
    );
    let missing = step(&h.engine, &tx, 1, "kv.put", json!({ "value": 1 })).await;
    assert_eq!(
        missing.hint.unwrap().text,
        "Hint: Required field 'key' is missing from the arguments. Provide 'key' explicitly."
    );
}
