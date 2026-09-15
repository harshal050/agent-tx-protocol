//! JSON report schema (`schema_version` 1).
//!
//! Mirrored by the zod schema in `packages/benchmarks`; keep them in sync.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct RunConfig {
    pub quick: bool,
    pub llm_step_ms: u64,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub generated_at_ms: u64,
    pub harness_version: &'static str,
    pub environment: Environment,
    pub config: RunConfig,
    pub methodology: Methodology,
    pub simulation: Vec<ScenarioResult>,
    pub step_latency: Vec<LatencyResult>,
    pub snapshot_restore: Vec<RestorePoint>,
    pub commit: Vec<CommitPoint>,
    pub error_cleaner: Vec<CleanerResult>,
    pub throughput: Vec<ThroughputPoint>,
}

impl Report {
    pub fn new(environment: Environment, config: RunConfig) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generated_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            harness_version: env!("CARGO_PKG_VERSION"),
            environment,
            config,
            methodology: Methodology::default(),
            simulation: Vec::new(),
            step_latency: Vec::new(),
            snapshot_restore: Vec::new(),
            commit: Vec::new(),
            error_cleaner: Vec::new(),
            throughput: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Environment {
    pub os: String,
    pub arch: String,
    pub cpu_model: Option<String>,
    pub logical_cores: usize,
    pub memory_gb: Option<f64>,
    pub rustc: Option<String>,
    pub build_profile: &'static str,
    pub git_sha: Option<String>,
    pub git_dirty: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct Methodology {
    pub agent_model: &'static str,
    pub baseline_policy: &'static str,
    pub agenttx_policy: &'static str,
    pub token_estimate: &'static str,
    pub modeled_latency: &'static str,
}

impl Default for Methodology {
    fn default() -> Self {
        Self {
            agent_model: "Deterministic scripted agent (no LLM). It fixes the root-cause step once any feedback in its context names the offending field, and follows the step the runner asks for next.",
            baseline_policy: "Common ReAct-style loop without a transaction layer: append the raw error to the context, retry the failing step up to 3 times, then restart from step 1 (up to 3 restarts). Each tool call auto-commits and side effects fire immediately. Duplicate-key errors on replay are treated as already done.",
            agenttx_policy: "Same agent and tools through the AgentTx engine with the default rollback policy (local depth 2, 1 global reset, replay budget factor 2.0). The agent receives only the Clean Hint.",
            token_estimate: "Tokens are estimated as ceil(bytes / 4).",
            modeled_latency: "Modeled end-to-end latency = tool calls × assumed LLM latency per call + measured harness wall time.",
        }
    }
}

/// Latency distribution. All values in microseconds.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Summary {
    pub samples: usize,
    pub mean_us: f64,
    pub min_us: f64,
    pub p50_us: f64,
    pub p90_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
}

#[derive(Debug, Serialize)]
pub struct ScenarioResult {
    pub id: String,
    pub kind: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub steps: u32,
    pub root_step: Option<u32>,
    pub failing_step: u32,
    pub baseline: RunMetrics,
    pub agenttx: RunMetrics,
}

#[derive(Debug, Serialize)]
pub struct RunMetrics {
    pub succeeded: bool,
    pub outcome: &'static str,
    pub tool_executions: u32,
    pub failed_executions: u32,
    /// Executions beyond one clean pass over all steps.
    pub extra_executions: u32,
    pub context_feedback_bytes: u64,
    pub context_feedback_tokens_est: u64,
    pub effects_dispatched: u32,
    pub duplicate_side_effects: u32,
    /// Effects dispatched by a run that did not succeed.
    pub leaked_side_effects: u32,
    /// Files left in the sandbox that the final state should not contain.
    pub orphaned_files: u32,
    pub wall_time_ms: f64,
    pub modeled_latency_ms: f64,
    pub actions: BTreeMap<String, u32>,
    pub events: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LatencyResult {
    pub mode: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub summary: Summary,
}

#[derive(Debug, Serialize)]
pub struct RestorePoint {
    pub journal_entries: u32,
    pub restore: Summary,
    pub forward_writes: Summary,
}

#[derive(Debug, Serialize)]
pub struct CommitPoint {
    pub keys: u32,
    pub commit: Summary,
}

#[derive(Debug, Serialize)]
pub struct CleanerResult {
    pub id: &'static str,
    pub label: &'static str,
    pub runtime: &'static str,
    /// The raw error text as received by the agent.
    pub raw: &'static str,
    pub raw_bytes: usize,
    pub raw_lines: usize,
    pub hint_bytes: usize,
    pub reduction_pct: f64,
    pub hint: String,
    pub rule: Option<&'static str>,
    pub category: &'static str,
    pub key: Option<String>,
    pub parse: Summary,
}

#[derive(Debug, Serialize)]
pub struct ThroughputPoint {
    pub concurrency: usize,
    pub transactions: usize,
    pub steps_per_transaction: u32,
    pub elapsed_ms: f64,
    pub transactions_per_sec: f64,
    pub steps_per_sec: f64,
}
