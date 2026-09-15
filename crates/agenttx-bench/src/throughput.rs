//! Concurrent transaction throughput through the in-process engine.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::bail;
use serde_json::json;

use agenttx::config::EngineConfig;
use agenttx::engine::{Engine, StepRequest, StepStatus, builtin_tools};
use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
use agenttx::storage::{RocksStore, StoreOptions};

use crate::fixtures;
use crate::report::{RunConfig, ThroughputPoint};

const STEPS_PER_TX: u32 = 20;

pub async fn run(config: &RunConfig) -> anyhow::Result<Vec<ThroughputPoint>> {
    let total_transactions = if config.quick { 64 } else { 512 };
    let mut points = Vec::new();
    for concurrency in [1usize, 4, 16, 64] {
        let db = tempfile::tempdir()?;
        let sandbox = tempfile::tempdir()?;
        let store = Arc::new(RocksStore::open(db.path(), StoreOptions::default())?);
        let undo = UndoRegistry::new();
        builtin_tools::register_builtin_undo_factories(&undo);
        let engine = Arc::new(Engine::new(
            store,
            fixtures::registry(sandbox.path()),
            undo,
            Arc::new(OutboxDispatcher::new(db.path().join("outbox.jsonl"))),
            EngineConfig {
                tx_idle_timeout: None,
                ..EngineConfig::default()
            },
        ));

        let per_worker = (total_transactions / concurrency).max(1);
        let started = Instant::now();
        let workers: Vec<_> = (0..concurrency)
            .map(|worker| {
                let engine = Arc::clone(&engine);
                tokio::spawn(async move {
                    for t in 0..per_worker {
                        let tx = engine.begin("bench", HashMap::new(), None).await?.tx_id;
                        for step in 1..=STEPS_PER_TX {
                            let outcome = engine
                                .execute_step(StepRequest {
                                    tx_id: tx.clone(),
                                    step_id: step,
                                    tool_name: "kv.put".into(),
                                    arguments_json: json!({
                                        "key": format!("w{worker}/t{t}/s{step}"),
                                        "value": step
                                    })
                                    .to_string(),
                                    raw_context: String::new(),
                                })
                                .await?;
                            if outcome.status != StepStatus::Success {
                                bail!("step failed: {:?}", outcome.hint);
                            }
                        }
                        engine.commit(&tx).await?;
                    }
                    Ok::<_, anyhow::Error>(())
                })
            })
            .collect();
        for worker in workers {
            worker.await??;
        }
        let elapsed = started.elapsed().as_secs_f64();
        let transactions = per_worker * concurrency;
        points.push(ThroughputPoint {
            concurrency,
            transactions,
            steps_per_transaction: STEPS_PER_TX,
            elapsed_ms: elapsed * 1000.0,
            transactions_per_sec: transactions as f64 / elapsed,
            steps_per_sec: (transactions as f64 * f64::from(STEPS_PER_TX)) / elapsed,
        });
    }
    Ok(points)
}
