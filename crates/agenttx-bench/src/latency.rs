//! Per-step latency: direct tool call vs AgentTx engine vs AgentTx over gRPC.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::bail;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use agenttx::config::EngineConfig;
use agenttx::engine::{Engine, StepRequest, StepStatus, builtin_tools};
use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
use agenttx::proto::agent_tx_service_client::AgentTxServiceClient;
use agenttx::proto::{
    BeginTransactionRequest, CommitTransactionRequest, ExecuteStepRequest,
    StepStatus as ProtoStatus,
};
use agenttx::storage::{RocksStore, StoreOptions};

use crate::fixtures;
use crate::report::{LatencyResult, RunConfig};
use crate::stats::{micros, summarize};

/// Steps per transaction before committing and starting a new one.
const STEPS_PER_TX: u32 = 250;

pub async fn run(config: &RunConfig) -> anyhow::Result<Vec<LatencyResult>> {
    let (warmup, samples) = if config.quick {
        (100, 1_000)
    } else {
        (500, 5_000)
    };
    Ok(vec![
        direct(warmup, samples).await?,
        engine(warmup, samples).await?,
        grpc(warmup, samples).await?,
    ])
}

fn args(i: usize) -> serde_json::Value {
    json!({ "key": format!("bench/key-{}", i % 1_000), "value": { "i": i, "status": "ok" } })
}

async fn direct(warmup: usize, samples: usize) -> anyhow::Result<LatencyResult> {
    let dir = tempfile::tempdir()?;
    let mut options = rocksdb::Options::default();
    options.create_if_missing(true);
    let db = rocksdb::DB::open(&options, dir.path())?;

    let mut measured = Vec::with_capacity(samples);
    for i in 0..warmup + samples {
        let arguments = args(i);
        let started = Instant::now();
        let key = arguments["key"].as_str().unwrap_or_default();
        db.put(
            format!("kv/{key}"),
            serde_json::to_vec(&arguments["value"])?,
        )?;
        if i >= warmup {
            measured.push(micros(started.elapsed()));
        }
    }
    Ok(LatencyResult {
        mode: "direct",
        label: "Direct write (no protocol)",
        description: "Before: the tool's effect as one auto-committed RocksDB put. No transaction, snapshot, journal or rollback.",
        summary: summarize(&mut measured),
    })
}

fn build_engine(db: &std::path::Path, sandbox: &std::path::Path) -> anyhow::Result<Arc<Engine>> {
    let store = Arc::new(RocksStore::open(db, StoreOptions::default())?);
    let undo = UndoRegistry::new();
    builtin_tools::register_builtin_undo_factories(&undo);
    Ok(Arc::new(Engine::new(
        store,
        fixtures::registry(sandbox),
        undo,
        Arc::new(OutboxDispatcher::new(db.join("outbox.jsonl"))),
        EngineConfig {
            tx_idle_timeout: None,
            ..EngineConfig::default()
        },
    )))
}

async fn engine(warmup: usize, samples: usize) -> anyhow::Result<LatencyResult> {
    let db = tempfile::tempdir()?;
    let sandbox = tempfile::tempdir()?;
    let engine = build_engine(db.path(), sandbox.path())?;

    let mut tx = engine.begin("bench", HashMap::new(), None).await?.tx_id;
    let mut step = 1;
    let mut measured = Vec::with_capacity(samples);
    for i in 0..warmup + samples {
        if step > STEPS_PER_TX {
            engine.commit(&tx).await?;
            tx = engine.begin("bench", HashMap::new(), None).await?.tx_id;
            step = 1;
        }
        let request = StepRequest {
            tx_id: tx.clone(),
            step_id: step,
            tool_name: "kv.put".into(),
            arguments_json: args(i).to_string(),
            raw_context: String::new(),
        };
        let started = Instant::now();
        let outcome = engine.execute_step(request).await?;
        let elapsed = started.elapsed();
        if outcome.status != StepStatus::Success {
            bail!("engine step failed: {:?}", outcome.hint);
        }
        if i >= warmup {
            measured.push(micros(elapsed));
        }
        step += 1;
    }
    Ok(LatencyResult {
        mode: "engine",
        label: "AgentTx engine (in-process)",
        description: "After: snapshot pointer, journaled overlay write, dependency-graph update and output record, embedded as a library.",
        summary: summarize(&mut measured),
    })
}

async fn grpc(warmup: usize, samples: usize) -> anyhow::Result<LatencyResult> {
    let db = tempfile::tempdir()?;
    let sandbox = tempfile::tempdir()?;
    let engine = build_engine(db.path(), sandbox.path())?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(agenttx::grpc::serve(engine, listener, async {
        let _ = shutdown_rx.await;
    }));
    let mut client = AgentTxServiceClient::connect(format!("http://{addr}")).await?;

    let begin = || BeginTransactionRequest {
        agent_id: "bench".into(),
        ..Default::default()
    };
    let mut tx = client
        .begin_transaction(begin())
        .await?
        .into_inner()
        .transaction_id;
    let mut step = 1;
    let mut measured = Vec::with_capacity(samples);
    for i in 0..warmup + samples {
        if step > STEPS_PER_TX {
            client
                .commit_transaction(CommitTransactionRequest {
                    transaction_id: tx.clone(),
                })
                .await?;
            tx = client
                .begin_transaction(begin())
                .await?
                .into_inner()
                .transaction_id;
            step = 1;
        }
        let request = ExecuteStepRequest {
            transaction_id: tx.clone(),
            step_id: step,
            tool_name: "kv.put".into(),
            arguments_json: args(i).to_string(),
            raw_context: String::new(),
        };
        let started = Instant::now();
        let response = client.execute_step(request).await?.into_inner();
        let elapsed = started.elapsed();
        if response.status() != ProtoStatus::Success {
            bail!("gRPC step failed: {}", response.clean_hint);
        }
        if i >= warmup {
            measured.push(micros(elapsed));
        }
        step += 1;
    }

    let _ = shutdown_tx.send(());
    server.await??;
    Ok(LatencyResult {
        mode: "grpc",
        label: "AgentTx over gRPC (loopback)",
        description: "After, as a proxy: full ExecuteStep round-trip over HTTP/2 on localhost, including protobuf encoding and the engine.",
        summary: summarize(&mut measured),
    })
}
