//! Agent recovery simulation: the same scripted agent and tools, run through a
//! naive agent loop (baseline) and through the AgentTx engine.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::anyhow;
use async_trait::async_trait;
use serde_json::{Value, json};

use agenttx::config::EngineConfig;
use agenttx::engine::executor::resolve_references;
use agenttx::engine::{
    Engine, StepRecord, StepRequest, StepStatus, ToolContext, ToolError, ToolRegistry,
    builtin_tools,
};
use agenttx::ledger::{EffectDispatcher, StagedEffect, UndoRegistry};
use agenttx::storage::{RocksStore, StoreOptions};

use crate::fixtures;
use crate::report::{RunConfig, RunMetrics, ScenarioResult};

const BASELINE_MAX_RETRIES: u32 = 3;
const BASELINE_MAX_RESTARTS: u32 = 3;
const MAX_EVENTS: usize = 48;
const GOOD_CUSTOMER: &str = "c-1";
const BAD_CUSTOMER: &str = "c-404";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureKind {
    RootCause,
    Transient,
    Persistent,
}

impl FailureKind {
    fn id(self) -> &'static str {
        match self {
            FailureKind::RootCause => "root-cause",
            FailureKind::Transient => "transient",
            FailureKind::Persistent => "persistent",
        }
    }

    fn title(self) -> &'static str {
        match self {
            FailureKind::RootCause => "Silent root cause",
            FailureKind::Transient => "Transient failure",
            FailureKind::Persistent => "Unrecoverable failure",
        }
    }

    fn description(self) -> &'static str {
        match self {
            FailureKind::RootCause => {
                "An early step stores the wrong customer id. The mistake only surfaces many steps later as a foreign-key violation."
            }
            FailureKind::Transient => {
                "A payment call times out once and succeeds when tried again."
            }
            FailureKind::Persistent => {
                "A reconciliation step fails every time, so the task cannot succeed. Measures how each approach stops and what it leaves behind."
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Scenario {
    kind: FailureKind,
    steps: u32,
    root: u32,
    fail_at: u32,
}

impl Scenario {
    fn new(kind: FailureKind, steps: u32) -> Self {
        let root = (steps / 4).max(2);
        let fail_at = (steps * 3).div_ceil(4).max(root + 2);
        Self {
            kind,
            steps,
            root,
            fail_at,
        }
    }

    /// The agent's tool call for step `i`. `learned` is true once the agent's
    /// context contains feedback naming the offending field.
    fn plan(&self, i: u32, learned: bool) -> (&'static str, Value) {
        let customer = if self.kind == FailureKind::RootCause && !learned {
            BAD_CUSTOMER
        } else {
            GOOD_CUSTOMER
        };
        let root_ref = format!("${{steps.{}.value}}", self.root);
        match i {
            1 => (
                "record.insert",
                json!({ "table": "customers", "id": GOOD_CUSTOMER, "fields": { "name": "Acme GmbH" } }),
            ),
            _ if i == self.root => (
                "kv.put",
                json!({ "key": "session/customer", "value": customer }),
            ),
            _ if i == self.root + 1 => (
                "fs.write",
                json!({ "path": format!("drafts/{root_ref}.json"), "contents": "{\"status\":\"draft\"}" }),
            ),
            _ if i == self.fail_at => match self.kind {
                FailureKind::RootCause => (
                    "bench.insert_invoice",
                    json!({
                        "table": "invoices",
                        "id": "inv-1",
                        "fields": { "customer_id": root_ref, "amount": 1200 },
                        "references": { "customer_id": "customers" }
                    }),
                ),
                FailureKind::Transient => ("bench.flaky", json!({ "operation": "charge_card" })),
                FailureKind::Persistent => ("bench.broken", json!({ "operation": "reconcile" })),
            },
            _ if i.is_multiple_of(5) => (
                "effect.stage",
                json!({
                    "channel": "email",
                    "payload": { "template": "step-complete", "step": i },
                    "idempotency_key": format!("wf-{i}")
                }),
            ),
            _ if i.is_multiple_of(3) => (
                "fs.write",
                json!({ "path": format!("reports/step-{i}.txt"), "contents": format!("report for step {i}") }),
            ),
            _ => (
                "kv.put",
                json!({ "key": format!("wf/step-{i}"), "value": i }),
            ),
        }
    }

    /// Files a correct, completed run leaves in the sandbox.
    fn expected_files(&self) -> BTreeSet<String> {
        (1..=self.steps)
            .filter_map(|i| {
                let (tool, args) = self.plan(i, true);
                (tool == "fs.write").then(|| {
                    let path = args["path"].as_str().unwrap_or_default();
                    if path.contains("${") {
                        format!("drafts/{GOOD_CUSTOMER}.json")
                    } else {
                        path.to_owned()
                    }
                })
            })
            .collect()
    }
}

pub async fn run(config: &RunConfig) -> anyhow::Result<Vec<ScenarioResult>> {
    let sizes: &[u32] = if config.quick {
        &[8, 32]
    } else {
        &[8, 16, 32, 64]
    };
    let mut results = Vec::new();
    for kind in [
        FailureKind::RootCause,
        FailureKind::Transient,
        FailureKind::Persistent,
    ] {
        for &steps in sizes {
            let scenario = Scenario::new(kind, steps);
            let baseline = run_baseline(&scenario, config).await?;
            let agenttx = run_agenttx(&scenario, config).await?;
            results.push(ScenarioResult {
                id: format!("{}-{steps}", kind.id()),
                kind: kind.id(),
                title: kind.title(),
                description: kind.description(),
                steps,
                root_step: (kind == FailureKind::RootCause).then_some(scenario.root),
                failing_step: scenario.fail_at,
                baseline,
                agenttx,
            });
        }
    }
    Ok(results)
}

#[derive(Default)]
struct Tally {
    executions: u32,
    failed: u32,
    feedback_bytes: u64,
    actions: BTreeMap<String, u32>,
    events: Vec<String>,
}

impl Tally {
    fn event(&mut self, action: &str, detail: String) {
        *self.actions.entry(action.to_owned()).or_insert(0) += 1;
        if self.events.len() < MAX_EVENTS {
            self.events.push(detail);
        } else if self.events.len() == MAX_EVENTS {
            self.events.push("…".to_owned());
        }
    }
}

// ---------------------------------------------------------------------------
// Baseline: no transaction layer
// ---------------------------------------------------------------------------

async fn run_baseline(scenario: &Scenario, config: &RunConfig) -> anyhow::Result<RunMetrics> {
    let db = tempfile::tempdir()?;
    let sandbox = tempfile::tempdir()?;
    let store = Arc::new(RocksStore::open(db.path(), StoreOptions::default())?);
    let tools = fixtures::registry(sandbox.path());

    let mut tally = Tally::default();
    let mut dispatched = Vec::new();
    let mut learned = false;
    let mut restarts = 0;
    let mut seq = 0u64;
    let started = Instant::now();

    let succeeded = 'run: loop {
        let mut outputs: BTreeMap<u32, StepRecord> = BTreeMap::new();
        let mut step = 1;
        while step <= scenario.steps {
            let mut retries = 0;
            loop {
                let (tool_name, args) = scenario.plan(step, learned);
                tally.executions += 1;
                seq += 1;
                let result = baseline_call(
                    &store,
                    &tools,
                    tool_name,
                    &args,
                    step,
                    &outputs,
                    seq,
                    &mut dispatched,
                )
                .await?;
                match result {
                    Ok(output) => {
                        outputs.insert(step, record(step, tool_name, args, output));
                        break;
                    }
                    Err(error) => {
                        tally.failed += 1;
                        tally.feedback_bytes += error.len() as u64;
                        learned |= error.contains("customer_id");
                        if error.contains("already exists") {
                            tally.event(
                                "skip_duplicate",
                                format!("step {step}: duplicate on replay, treated as done"),
                            );
                            let output = args.clone();
                            outputs.insert(step, record(step, tool_name, args, output));
                            break;
                        }
                        retries += 1;
                        if retries > BASELINE_MAX_RETRIES {
                            restarts += 1;
                            if restarts > BASELINE_MAX_RESTARTS {
                                tally.event("give_up", format!("step {step}: gave up after {BASELINE_MAX_RESTARTS} restarts"));
                                break 'run false;
                            }
                            tally.event("restart", format!("step {step}: restart from step 1"));
                            continue 'run;
                        }
                        tally.event(
                            "retry",
                            format!("step {step}: retry with raw error in context"),
                        );
                    }
                }
            }
            step += 1;
        }
        break true;
    };

    let outcome = if succeeded { "completed" } else { "gave up" };
    Ok(finish(
        tally,
        succeeded,
        outcome,
        &dispatched,
        sandbox.path(),
        scenario,
        started.elapsed(),
        config,
    ))
}

fn record(step: u32, tool_name: &str, arguments: Value, output: Value) -> StepRecord {
    StepRecord {
        step_id: step,
        tool_name: tool_name.to_owned(),
        arguments,
        output,
    }
}

/// One tool call with auto-commit semantics: whatever the tool wrote is
/// committed, compensations are unavailable and side effects fire immediately.
#[allow(clippy::too_many_arguments)]
async fn baseline_call(
    store: &Arc<RocksStore>,
    tools: &ToolRegistry,
    tool_name: &str,
    args: &Value,
    step: u32,
    outputs: &BTreeMap<u32, StepRecord>,
    seq: u64,
    dispatched: &mut Vec<StagedEffect>,
) -> anyhow::Result<Result<Value, String>> {
    let resolved = match resolve_references(args, step, outputs) {
        Ok(resolved) => resolved.value,
        Err(failure) => return Ok(Err(failure.message)),
    };
    let tool = tools
        .get(tool_name)
        .ok_or_else(|| anyhow!("unknown tool {tool_name}"))?;
    let tx = format!("baseline-{seq}");
    let ctx = ToolContext::new(store.scoped(&tx)?, step);
    let result = tool.execute(&ctx, resolved).await;
    store.commit(&tx)?;
    drop(ctx.take_undo());
    dispatched.extend(ctx.take_staged());
    match result {
        Ok(output) => Ok(Ok(output)),
        Err(ToolError::Failed(message)) => Ok(Err(message)),
        Err(ToolError::Internal(err)) => Err(err.into()),
    }
}

// ---------------------------------------------------------------------------
// AgentTx
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CollectingDispatcher(Mutex<Vec<StagedEffect>>);

impl CollectingDispatcher {
    fn take(&self) -> Vec<StagedEffect> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

#[async_trait]
impl EffectDispatcher for CollectingDispatcher {
    async fn dispatch(&self, _tx_id: &str, effect: &StagedEffect) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(effect.clone());
        Ok(())
    }
}

async fn run_agenttx(scenario: &Scenario, config: &RunConfig) -> anyhow::Result<RunMetrics> {
    let db = tempfile::tempdir()?;
    let sandbox = tempfile::tempdir()?;
    let store = Arc::new(RocksStore::open(db.path(), StoreOptions::default())?);
    let undo = UndoRegistry::new();
    builtin_tools::register_builtin_undo_factories(&undo);
    let dispatcher = Arc::new(CollectingDispatcher::default());
    let engine = Engine::new(
        store,
        fixtures::registry(sandbox.path()),
        undo,
        dispatcher.clone(),
        EngineConfig {
            tx_idle_timeout: None,
            ..EngineConfig::default()
        },
    );

    let mut tally = Tally::default();
    let mut learned = false;
    let started = Instant::now();
    let tx = engine
        .begin("bench-agent", HashMap::new(), None)
        .await?
        .tx_id;
    let mut next = 1;

    let succeeded = loop {
        if next > scenario.steps {
            engine.commit(&tx).await?;
            break true;
        }
        let (tool_name, args) = scenario.plan(next, learned);
        tally.executions += 1;
        let outcome = engine
            .execute_step(StepRequest {
                tx_id: tx.clone(),
                step_id: next,
                tool_name: tool_name.to_owned(),
                arguments_json: args.to_string(),
                raw_context: String::new(),
            })
            .await?;
        if outcome.status == StepStatus::Success {
            next = outcome.next_step;
            continue;
        }
        tally.failed += 1;
        let hint = outcome.hint.as_ref().map_or("", |h| h.text.as_str());
        tally.feedback_bytes += hint.len() as u64;
        learned |= hint.contains("customer_id");
        let strategy = outcome.strategy.as_str().to_ascii_lowercase();
        if outcome.status == StepStatus::Failed {
            tally.event(
                &strategy,
                format!("step {next}: abort, all effects compensated"),
            );
            break false;
        }
        tally.event(
            &strategy,
            format!(
                "step {next}: {strategy} → resume at step {}",
                outcome.next_step
            ),
        );
        next = outcome.next_step;
    };

    let outcome = if succeeded { "committed" } else { "aborted" };
    let effects = dispatcher.take();
    Ok(finish(
        tally,
        succeeded,
        outcome,
        &effects,
        sandbox.path(),
        scenario,
        started.elapsed(),
        config,
    ))
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn finish(
    tally: Tally,
    succeeded: bool,
    outcome: &'static str,
    effects: &[StagedEffect],
    sandbox: &Path,
    scenario: &Scenario,
    wall: Duration,
    config: &RunConfig,
) -> RunMetrics {
    let unique: BTreeSet<&str> = effects.iter().map(|e| e.idempotency_key.as_str()).collect();
    let dispatched = effects.len() as u32;
    let expected = if succeeded {
        scenario.expected_files()
    } else {
        BTreeSet::new()
    };
    let orphaned = list_files(sandbox)
        .into_iter()
        .filter(|f| !expected.contains(f))
        .count() as u32;
    let wall_time_ms = wall.as_secs_f64() * 1000.0;
    RunMetrics {
        succeeded,
        outcome,
        tool_executions: tally.executions,
        failed_executions: tally.failed,
        extra_executions: tally.executions.saturating_sub(scenario.steps),
        context_feedback_bytes: tally.feedback_bytes,
        context_feedback_tokens_est: tally.feedback_bytes.div_ceil(4),
        effects_dispatched: dispatched,
        duplicate_side_effects: dispatched - unique.len() as u32,
        leaked_side_effects: if succeeded { 0 } else { dispatched },
        orphaned_files: orphaned,
        wall_time_ms,
        modeled_latency_ms: f64::from(tally.executions) * config.llm_step_ms as f64 + wall_time_ms,
        actions: tally.actions,
        events: tally.events,
    }
}

fn list_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Ok(rel) = path.strip_prefix(root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if !rel.ends_with(".agenttx-tmp") {
                    out.push(rel);
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_positions_do_not_collide() {
        for steps in [8, 16, 32, 64] {
            let s = Scenario::new(FailureKind::RootCause, steps);
            assert!(s.root >= 2);
            assert!(s.fail_at > s.root + 1);
            assert!(s.fail_at <= steps);
        }
    }

    #[test]
    fn expected_files_resolve_draft_reference() {
        let s = Scenario::new(FailureKind::RootCause, 8);
        let files = s.expected_files();
        assert!(files.contains("drafts/c-1.json"));
        assert!(files.iter().all(|f| !f.contains("${")));
    }

    #[tokio::test]
    async fn agenttx_recovers_root_cause_without_leaks() {
        let config = RunConfig {
            quick: true,
            llm_step_ms: 1000,
        };
        let s = Scenario::new(FailureKind::RootCause, 8);
        let after = run_agenttx(&s, &config).await.unwrap();
        assert!(after.succeeded);
        assert_eq!(after.duplicate_side_effects, 0);
        assert_eq!(after.orphaned_files, 0);
        assert_eq!(after.actions.get("dependency_jump"), Some(&1));

        let before = run_baseline(&s, &config).await.unwrap();
        assert!(before.succeeded);
        assert!(before.duplicate_side_effects > 0 || before.orphaned_files > 0);
        assert!(before.context_feedback_bytes > after.context_feedback_bytes);
    }

    #[tokio::test]
    async fn persistent_failure_terminates_in_both_modes() {
        let config = RunConfig {
            quick: true,
            llm_step_ms: 1000,
        };
        let s = Scenario::new(FailureKind::Persistent, 8);
        let after = run_agenttx(&s, &config).await.unwrap();
        assert!(!after.succeeded);
        assert_eq!(after.leaked_side_effects, 0);
        assert_eq!(after.orphaned_files, 0);
        let before = run_baseline(&s, &config).await.unwrap();
        assert!(!before.succeeded);
        assert!(before.tool_executions > after.tool_executions);
    }
}
