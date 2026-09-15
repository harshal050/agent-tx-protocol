//! Transaction engine: orchestrates snapshots, tool execution, error
//! cleaning, rollback decisions, Saga compensation and commit.
//!
//! Every transaction is guarded by its own `tokio::sync::Mutex`, so steps of
//! one transaction are strictly serialized while different transactions run
//! fully in parallel. The transaction table is a lock-free `DashMap`.

pub mod builtin_tools;
pub mod executor;
pub mod state_machine;
pub mod transaction;

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::{Value, json};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub use executor::{StepExecutor, Tool, ToolContext, ToolError, ToolRegistry};
pub use state_machine::{
    EscalationReason, RollbackController, RollbackDecision, RollbackPolicy, RollbackStrategy,
    TxStatus,
};
pub use transaction::{ContextEntry, StepRecord, Transaction};

use crate::config::EngineConfig;
use crate::errors::{AgentTxError, Result};
use crate::ledger::{CompensationReport, Dependency, EffectDispatcher, SagaLedger, UndoRegistry};
use crate::parser::{CleanHint, ErrorCleaner};
use crate::storage::{RocksStore, SnapshotId, TxRecord};
use crate::storage::snapshot::now_ms;

/// Maximum accepted `agent_id` length.
pub const MAX_AGENT_ID_LEN: usize = 256;

/// Outcome class of one step execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepStatus {
    Success,
    RollbackTriggered,
    Failed,
}

/// Input for [`Engine::execute_step`].
#[derive(Debug, Clone, Default)]
pub struct StepRequest {
    pub tx_id: String,
    pub step_id: u32,
    pub tool_name: String,
    pub arguments_json: String,
    pub raw_context: String,
}

/// Result of [`Engine::execute_step`].
#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub status: StepStatus,
    pub output: Option<Value>,
    pub hint: Option<CleanHint>,
    pub strategy: RollbackStrategy,
    /// First step whose effects were undone (0 = initial state).
    pub rollback_to_step: u32,
    /// Step the agent must send next (0 once the transaction has ended).
    pub next_step: u32,
    pub constraints: Vec<String>,
    pub context_reset: bool,
    pub rollback_latency: Duration,
    pub abort_reason: Option<String>,
    pub compensation_failures: Vec<String>,
    /// Dependencies recorded for this step.
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeginOutcome {
    pub tx_id: String,
    pub next_step: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub steps_committed: u32,
    pub keys_published: usize,
    pub effects_dispatched: u32,
    pub dispatch_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackOutcome {
    /// `true` if the whole transaction was aborted.
    pub aborted: bool,
    pub next_step: u32,
    pub compensations_run: u32,
    pub compensation_failures: Vec<String>,
}

/// Read-only view of a live transaction.
#[derive(Debug, Clone)]
pub struct TransactionView {
    pub tx_id: String,
    pub agent_id: String,
    pub status: TxStatus,
    pub next_step: u32,
    pub steps_completed: usize,
    pub constraints: Vec<String>,
    pub global_resets: u32,
    pub context: Vec<ContextEntry>,
    pub pending_effects: usize,
    pub created_at_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub transactions: usize,
    pub compensations_run: u32,
    pub failures: Vec<String>,
}

enum StepAttempt {
    Succeeded {
        arguments: Value,
        output: Value,
        dependencies: Vec<Dependency>,
    },
    Failed {
        error: String,
        dependencies: Vec<Dependency>,
    },
}

type TxHandle = Arc<Mutex<Transaction>>;

/// The AgentTx transaction engine.
pub struct Engine {
    store: Arc<RocksStore>,
    tools: ToolRegistry,
    undo_registry: UndoRegistry,
    dispatcher: Arc<dyn EffectDispatcher>,
    cleaner: &'static ErrorCleaner,
    executor: StepExecutor,
    config: EngineConfig,
    transactions: DashMap<String, TxHandle>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("store", &self.store)
            .field("tools", &self.tools)
            .field("active_transactions", &self.transactions.len())
            .finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(
        store: Arc<RocksStore>,
        tools: ToolRegistry,
        undo_registry: UndoRegistry,
        dispatcher: Arc<dyn EffectDispatcher>,
        config: EngineConfig,
    ) -> Self {
        Self {
            store,
            tools,
            undo_registry,
            dispatcher,
            cleaner: ErrorCleaner::global(),
            executor: StepExecutor::new(config.step_timeout),
            config,
            transactions: DashMap::new(),
        }
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn store(&self) -> &Arc<RocksStore> {
        &self.store
    }

    pub fn tools(&self) -> &ToolRegistry {
        &self.tools
    }

    pub fn active_transactions(&self) -> usize {
        self.transactions.len()
    }

    fn handle(&self, tx_id: &str) -> Result<TxHandle> {
        self.transactions
            .get(tx_id)
            .map(|h| Arc::clone(h.value()))
            .ok_or_else(|| AgentTxError::TransactionNotFound(tx_id.to_owned()))
    }

    // =====================================================================
    // Begin
    // =====================================================================

    /// Opens a transaction and captures its Step-0 snapshot.
    pub async fn begin(
        &self,
        agent_id: &str,
        metadata: HashMap<String, String>,
        policy: Option<RollbackPolicy>,
    ) -> Result<BeginOutcome> {
        if agent_id.len() > MAX_AGENT_ID_LEN {
            return Err(AgentTxError::InvalidArgument(format!(
                "agent_id exceeds {MAX_AGENT_ID_LEN} bytes"
            )));
        }
        let policy = policy.unwrap_or(self.config.default_policy);
        policy.validate()?;

        let tx_id = uuid::Uuid::new_v4().to_string();
        self.store.register_transaction(&TxRecord {
            tx_id: tx_id.clone(),
            agent_id: agent_id.to_owned(),
            created_at_ms: now_ms(),
        })?;
        self.store.create_snapshot(&tx_id, 0)?;

        let saga = SagaLedger::new(&tx_id, Arc::clone(&self.store), self.config.compensation_retries);
        let tx = Transaction::new(tx_id.clone(), agent_id.to_owned(), metadata, policy, saga);
        self.transactions.insert(tx_id.clone(), Arc::new(Mutex::new(tx)));
        tracing::info!(tx = %tx_id, agent = %agent_id, "transaction started");
        Ok(BeginOutcome { tx_id, next_step: 1 })
    }

    // =====================================================================
    // Execute
    // =====================================================================

    /// Executes one step. Agent-attributable failures are handled by the
    /// rollback controller and returned as a successful call carrying
    /// `RollbackTriggered` or `Failed`; only infrastructure/protocol problems
    /// produce `Err`.
    pub async fn execute_step(&self, req: StepRequest) -> Result<StepOutcome> {
        let handle = self.handle(&req.tx_id)?;
        let mut guard = handle.lock().await;
        let tx = &mut *guard;
        tx.ensure_active()?;
        if req.step_id != tx.next_step {
            return Err(AgentTxError::StepOutOfOrder {
                expected: tx.next_step,
                got: req.step_id,
            });
        }
        tx.touch();
        tx.transition(TxStatus::Executing)?;

        match self.execute_locked(tx, &req).await {
            Ok(outcome) => Ok(outcome),
            Err(err) => {
                tracing::error!(tx = %tx.id, step = req.step_id, error = %err, "step aborted by infrastructure error");
                if tx.status == TxStatus::Executing {
                    // Undo partial effects so the step can be retried cleanly.
                    tx.status = TxStatus::RollingBack;
                    match self.rewind_locked(tx, req.step_id).await {
                        Ok(_) => tx.status = TxStatus::Active,
                        Err(rewind_err) => {
                            tracing::error!(tx = %tx.id, error = %rewind_err, "failed to rewind after error");
                            self.fail_hard(tx).await;
                        }
                    }
                } else if tx.status == TxStatus::RollingBack {
                    self.fail_hard(tx).await;
                }
                Err(err)
            }
        }
    }

    async fn execute_locked(&self, tx: &mut Transaction, req: &StepRequest) -> Result<StepOutcome> {
        let step = req.step_id;
        let scoped = self.store.scoped(&tx.id)?;
        scoped.create_snapshot(step)?;
        tx.controller.observe_step(step);

        match self.run_step(tx, req).await? {
            StepAttempt::Succeeded {
                arguments,
                output,
                dependencies,
            } => {
                scoped.put_output(step, &serde_json::to_vec(&output)?)?;
                tx.graph.record_output(step, &output);
                tx.steps.insert(
                    step,
                    StepRecord {
                        step_id: step,
                        tool_name: req.tool_name.clone(),
                        arguments,
                        output: output.clone(),
                    },
                );
                tx.context.push(ContextEntry {
                    step_id: step,
                    tool_name: req.tool_name.clone(),
                    raw_context: req.raw_context.clone(),
                });
                tx.next_step = step + 1;
                tx.transition(TxStatus::Active)?;
                tracing::debug!(tx = %tx.id, step, tool = %req.tool_name, "step succeeded");
                Ok(StepOutcome {
                    status: StepStatus::Success,
                    output: Some(output),
                    hint: None,
                    strategy: RollbackStrategy::None,
                    rollback_to_step: 0,
                    next_step: tx.next_step,
                    constraints: tx.constraints.clone(),
                    context_reset: false,
                    rollback_latency: Duration::ZERO,
                    abort_reason: None,
                    compensation_failures: Vec::new(),
                    dependencies,
                })
            }
            StepAttempt::Failed {
                error,
                dependencies,
            } => self.handle_failure(tx, step, &error, dependencies).await,
        }
    }

    async fn run_step(&self, tx: &mut Transaction, req: &StepRequest) -> Result<StepAttempt> {
        let step = req.step_id;
        let tool_name = req.tool_name.as_str();

        let args: Value = if req.arguments_json.trim().is_empty() {
            json!({})
        } else {
            match serde_json::from_str(&req.arguments_json) {
                Ok(value) => value,
                Err(e) => {
                    tx.graph.add_step(step, tool_name, &Value::Null, &[]);
                    return Ok(StepAttempt::Failed {
                        error: format!("invalid arguments JSON: {e}"),
                        dependencies: Vec::new(),
                    });
                }
            }
        };

        let Some(tool) = self.tools.get(tool_name) else {
            tx.graph.add_step(step, tool_name, &args, &[]);
            return Ok(StepAttempt::Failed {
                error: format!(
                    "unknown tool '{tool_name}'; registered tools: {}",
                    self.tools.names().join(", ")
                ),
                dependencies: Vec::new(),
            });
        };

        let resolved = match executor::resolve_references(&args, step, &tx.steps) {
            Ok(resolved) => resolved,
            Err(failure) => {
                let dependencies = tx.graph.add_step(step, tool_name, &args, &failure.dependencies);
                return Ok(StepAttempt::Failed {
                    error: failure.message,
                    dependencies,
                });
            }
        };
        let dependencies =
            tx.graph
                .add_step(step, tool_name, &resolved.value, &resolved.dependencies);

        let ctx = Arc::new(ToolContext::new(self.store.scoped(&tx.id)?, step));
        let result = self
            .executor
            .run(tool, Arc::clone(&ctx), resolved.value.clone())
            .await;

        // Side-effect declarations are kept even when the tool failed, so
        // partially applied effects are compensated by the rewind.
        for action in ctx.take_undo() {
            tx.saga.record(step, action).await?;
        }
        for effect in ctx.take_staged() {
            tx.staging.stage(effect);
        }

        match result {
            Ok(output) => Ok(StepAttempt::Succeeded {
                arguments: resolved.value,
                output,
                dependencies,
            }),
            Err(ToolError::Failed(error)) => Ok(StepAttempt::Failed {
                error,
                dependencies,
            }),
            Err(ToolError::Internal(err)) => Err(err),
        }
    }

    async fn handle_failure(
        &self,
        tx: &mut Transaction,
        step: u32,
        raw_error: &str,
        dependencies: Vec<Dependency>,
    ) -> Result<StepOutcome> {
        let hint = self.cleaner.clean(raw_error);
        tx.add_constraint(&hint.text, self.config.max_constraints);
        let decision = tx.controller.decide(step, hint.key.as_deref(), &tx.graph);
        tracing::info!(
            tx = %tx.id,
            step,
            category = %hint.category,
            rule = hint.rule.unwrap_or("fallback"),
            key = hint.key.as_deref().unwrap_or(""),
            strategy = decision.strategy().as_str(),
            target = decision.target_step(),
            "step failed"
        );

        tx.transition(TxStatus::RollingBack)?;
        let started = Instant::now();
        let strategy = decision.strategy();

        let (status, report, rollback_to_step, next_step, context_reset, abort_reason) =
            match decision {
                RollbackDecision::Abort { reason } => {
                    let report = self.abort_locked(tx, TxStatus::Failed).await?;
                    (StepStatus::Failed, report, 0, 0, true, Some(reason))
                }
                RollbackDecision::GlobalReset { .. } => {
                    let report = self.rewind_locked(tx, 0).await?;
                    tx.transition(TxStatus::Active)?;
                    (StepStatus::RollbackTriggered, report, 0, 1, true, None)
                }
                RollbackDecision::DependencyJump { target_step, .. }
                | RollbackDecision::LocalBacktrack { target_step, .. } => {
                    let report = self.rewind_locked(tx, target_step).await?;
                    tx.transition(TxStatus::Active)?;
                    (StepStatus::RollbackTriggered, report, target_step, target_step, false, None)
                }
            };

        Ok(StepOutcome {
            status,
            output: None,
            hint: Some(hint),
            strategy,
            rollback_to_step,
            next_step,
            constraints: tx.constraints.clone(),
            context_reset,
            rollback_latency: started.elapsed(),
            abort_reason,
            compensation_failures: report.failures,
            dependencies,
        })
    }

    /// Rewinds to the state before `target_step` (0 = initial state):
    /// compensates Saga actions N→target, discards staged effects, restores
    /// the storage snapshot and truncates in-memory history.
    async fn rewind_locked(&self, tx: &mut Transaction, target_step: u32) -> Result<CompensationReport> {
        let report = tx.saga.compensate_from(target_step).await;
        let discarded = tx.staging.discard_from(target_step);

        let store = Arc::clone(&self.store);
        let id = SnapshotId::new(tx.id.clone(), target_step);
        let stats = tokio::task::spawn_blocking(move || store.restore_snapshot(&id)).await??;
        tx.truncate_from(target_step);
        tracing::debug!(
            tx = %tx.id,
            target_step,
            reverted = stats.entries_reverted,
            restore_us = stats.elapsed.as_micros() as u64,
            compensated = report.succeeded,
            discarded,
            "rewound"
        );
        Ok(report)
    }

    /// Compensates everything, discards storage and removes the transaction.
    async fn abort_locked(&self, tx: &mut Transaction, final_status: TxStatus) -> Result<CompensationReport> {
        if tx.status != TxStatus::RollingBack {
            tx.transition(TxStatus::RollingBack)?;
        }
        let report = tx.saga.compensate_all().await;
        tx.staging.discard_all();

        let store = Arc::clone(&self.store);
        let id = tx.id.clone();
        let clean = report.is_clean();
        tokio::task::spawn_blocking(move || {
            if clean { store.discard(&id) } else { store.discard_state(&id) }
        })
        .await??;

        tx.truncate_from(0);
        tx.next_step = 0;
        tx.transition(final_status)?;
        self.transactions.remove(&tx.id);
        if clean {
            tracing::info!(tx = %tx.id, status = %final_status, compensated = report.succeeded, "transaction aborted");
        } else {
            tracing::error!(tx = %tx.id, failures = ?report.failures, "transaction aborted with failed compensations; saga logs retained");
        }
        Ok(report)
    }

    /// Last-resort cleanup when a rewind itself failed.
    async fn fail_hard(&self, tx: &mut Transaction) {
        tx.status = TxStatus::RollingBack;
        if let Err(err) = self.abort_locked(tx, TxStatus::Failed).await {
            tracing::error!(tx = %tx.id, error = %err, "abort after failed rewind also failed; recovery will retry on restart");
            tx.status = TxStatus::Failed;
            self.transactions.remove(&tx.id);
        }
    }

    // =====================================================================
    // Commit / rollback
    // =====================================================================

    /// Atomically publishes the transaction's state, then dispatches staged
    /// non-reversible effects.
    ///
    /// A write-write conflict aborts the transaction (with compensation) and
    /// returns [`AgentTxError::WriteConflict`].
    pub async fn commit(&self, tx_id: &str) -> Result<CommitOutcome> {
        let handle = self.handle(tx_id)?;
        let mut guard = handle.lock().await;
        let tx = &mut *guard;
        tx.ensure_active()?;
        tx.transition(TxStatus::Committing)?;

        let store = Arc::clone(&self.store);
        let id = tx.id.clone();
        let keys_published = match tokio::task::spawn_blocking(move || store.commit(&id)).await? {
            Ok(n) => n,
            Err(AgentTxError::WriteConflict { key }) => {
                tracing::warn!(tx = %tx.id, %key, "commit conflict; aborting");
                tx.transition(TxStatus::RollingBack)?;
                self.abort_locked(tx, TxStatus::Failed).await?;
                return Err(AgentTxError::WriteConflict { key });
            }
            Err(err) => {
                tx.transition(TxStatus::Active)?;
                return Err(err);
            }
        };

        tx.saga.forget_all().await;
        let steps_committed = tx.steps.len() as u32;
        let flush = tx.staging.flush(&tx.id, self.dispatcher.as_ref()).await;
        tx.transition(TxStatus::Committed)?;
        self.transactions.remove(&tx.id);
        if !flush.errors.is_empty() {
            tracing::error!(tx = %tx.id, errors = ?flush.errors, "committed effects failed to dispatch");
        }
        tracing::info!(tx = %tx.id, steps_committed, keys_published, effects = flush.dispatched, "transaction committed");
        Ok(CommitOutcome {
            steps_committed,
            keys_published,
            effects_dispatched: flush.dispatched,
            dispatch_errors: flush.errors,
        })
    }

    /// Rolls back on request.
    ///
    /// With `to_step = Some(n)` (`n > 0`) the transaction rewinds to just
    /// before step `n` and stays open; otherwise it is aborted entirely.
    pub async fn rollback(&self, tx_id: &str, to_step: Option<u32>, reason: &str) -> Result<RollbackOutcome> {
        let handle = self.handle(tx_id)?;
        let mut guard = handle.lock().await;
        let tx = &mut *guard;
        tx.ensure_active()?;
        tracing::info!(tx = %tx.id, ?to_step, %reason, "rollback requested");

        match to_step.filter(|s| *s > 0) {
            Some(step) => {
                if step >= tx.next_step {
                    return Err(AgentTxError::InvalidArgument(format!(
                        "to_step {step} must be lower than the next step {}",
                        tx.next_step
                    )));
                }
                tx.transition(TxStatus::RollingBack)?;
                let report = match self.rewind_locked(tx, step).await {
                    Ok(report) => report,
                    Err(err) => {
                        self.fail_hard(tx).await;
                        return Err(err);
                    }
                };
                tx.transition(TxStatus::Active)?;
                Ok(RollbackOutcome {
                    aborted: false,
                    next_step: tx.next_step,
                    compensations_run: report.succeeded,
                    compensation_failures: report.failures,
                })
            }
            None => {
                let report = self.abort_locked(tx, TxStatus::RolledBack).await?;
                Ok(RollbackOutcome {
                    aborted: true,
                    next_step: 0,
                    compensations_run: report.succeeded,
                    compensation_failures: report.failures,
                })
            }
        }
    }

    /// Snapshot of a live transaction's state.
    pub async fn get(&self, tx_id: &str) -> Result<TransactionView> {
        let handle = self.handle(tx_id)?;
        let tx = handle.lock().await;
        Ok(TransactionView {
            tx_id: tx.id.clone(),
            agent_id: tx.agent_id.clone(),
            status: tx.status,
            next_step: tx.next_step,
            steps_completed: tx.steps.len(),
            constraints: tx.constraints.clone(),
            global_resets: tx.controller.global_resets(),
            context: tx.context.clone(),
            pending_effects: tx.staging.len(),
            created_at_ms: tx.created_at_ms,
        })
    }

    // =====================================================================
    // Recovery and maintenance
    // =====================================================================

    /// Presumed-abort crash recovery: for every transaction left open on disk,
    /// rebuild and run its Saga compensations, then discard its state.
    ///
    /// Call once at startup, before serving traffic.
    pub async fn recover(&self) -> Result<RecoveryReport> {
        let mut report = RecoveryReport::default();
        for record in self.store.open_transactions()? {
            if self.transactions.contains_key(&record.tx_id) {
                continue;
            }
            report.transactions += 1;
            let (ledger, rebuild_errors) = SagaLedger::recover(
                &record.tx_id,
                Arc::clone(&self.store),
                &self.undo_registry,
                self.config.compensation_retries,
            )?;
            let compensation = ledger.compensate_all().await;
            report.compensations_run += compensation.succeeded;

            if rebuild_errors.is_empty() && compensation.is_clean() {
                self.store.discard(&record.tx_id)?;
                tracing::info!(tx = %record.tx_id, compensated = compensation.succeeded, "recovered interrupted transaction");
            } else {
                self.store.discard_state(&record.tx_id)?;
                let failures = rebuild_errors.into_iter().chain(compensation.failures);
                report
                    .failures
                    .extend(failures.map(|f| format!("{}: {f}", record.tx_id)));
                tracing::error!(tx = %record.tx_id, "recovery incomplete; saga logs retained");
            }
        }
        Ok(report)
    }

    /// Aborts transactions idle longer than the configured timeout.
    /// Transactions currently executing are never considered idle.
    pub async fn reap_idle(&self) -> usize {
        let Some(timeout) = self.config.tx_idle_timeout else {
            return 0;
        };
        let handles: Vec<TxHandle> = self
            .transactions
            .iter()
            .map(|e| Arc::clone(e.value()))
            .collect();
        let mut reaped = 0;
        for handle in handles {
            let Ok(mut tx) = handle.try_lock() else {
                continue;
            };
            if tx.status == TxStatus::Active && tx.last_activity.elapsed() >= timeout {
                match self.abort_locked(&mut tx, TxStatus::RolledBack).await {
                    Ok(_) => reaped += 1,
                    Err(err) => tracing::error!(tx = %tx.id, error = %err, "failed to reap idle transaction"),
                }
            }
        }
        reaped
    }

    /// Spawns the idle-transaction reaper. The task stops when the engine is
    /// dropped. Returns `None` when idle timeouts are disabled.
    pub fn spawn_reaper(self: &Arc<Self>) -> Option<JoinHandle<()>> {
        let timeout = self.config.tx_idle_timeout?;
        let period = (timeout / 4).clamp(Duration::from_millis(50), Duration::from_secs(60));
        let engine: Weak<Engine> = Arc::downgrade(self);
        Some(tokio::spawn(async move {
            let mut ticker = tokio::time::interval(period);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                ticker.tick().await;
                let Some(engine) = engine.upgrade() else { break };
                let reaped = engine.reap_idle().await;
                if reaped > 0 {
                    tracing::info!(reaped, "aborted idle transactions");
                }
            }
        }))
    }
}
