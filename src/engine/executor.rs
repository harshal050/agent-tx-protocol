//! Tool abstraction, registry, sandboxed step execution and `${steps.…}`
//! reference resolution.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use regex::Regex;
use serde_json::Value;

use super::transaction::StepRecord;
use crate::errors::AgentTxError;
use crate::ledger::{Dependency, DependencyKind, StagedEffect, UndoAction};
use crate::storage::TxStore;

/// Result of a tool invocation that did not succeed.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Agent-attributable failure: parsed into a Clean Hint and fed to the
    /// rollback controller. The message should be the raw error/stack trace.
    #[error("{0}")]
    Failed(String),
    /// Infrastructure failure: surfaced to the caller as an RPC error.
    #[error(transparent)]
    Internal(#[from] AgentTxError),
}

impl ToolError {
    pub fn failed(message: impl Into<String>) -> Self {
        ToolError::Failed(message.into())
    }
}

/// Per-step capabilities handed to a tool.
///
/// State access goes through the transaction's journaled overlay, so every
/// write is automatically covered by snapshot rewinds. External effects must
/// be declared via [`ToolContext::register_undo`] (reversible) or
/// [`ToolContext::stage_effect`] (non-reversible, deferred to commit).
pub struct ToolContext {
    store: TxStore,
    step_id: u32,
    undo: Mutex<Vec<Arc<dyn UndoAction>>>,
    staged: Mutex<Vec<StagedEffect>>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("tx_id", &self.store.tx_id())
            .field("step_id", &self.step_id)
            .finish_non_exhaustive()
    }
}

impl ToolContext {
    pub fn new(store: TxStore, step_id: u32) -> Self {
        Self {
            store,
            step_id,
            undo: Mutex::new(Vec::new()),
            staged: Mutex::new(Vec::new()),
        }
    }

    pub fn tx_id(&self) -> &str {
        self.store.tx_id()
    }

    pub fn step_id(&self) -> u32 {
        self.step_id
    }

    /// Transaction-scoped, journaled state store.
    pub fn store(&self) -> &TxStore {
        &self.store
    }

    /// Registers the compensation for a reversible effect. Register *before*
    /// performing the effect so partial failures are still compensated.
    pub fn register_undo(&self, action: Arc<dyn UndoAction>) {
        self.undo
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(action);
    }

    /// Stages a non-reversible effect for dispatch on commit. Returns its
    /// idempotency key (generated if not supplied).
    pub fn stage_effect(
        &self,
        channel: impl Into<String>,
        payload: Value,
        idempotency_key: Option<String>,
    ) -> String {
        let key = idempotency_key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.staged
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(StagedEffect {
                step_id: self.step_id,
                channel: channel.into(),
                payload,
                idempotency_key: key.clone(),
            });
        key
    }

    pub(crate) fn take_undo(&self) -> Vec<Arc<dyn UndoAction>> {
        std::mem::take(&mut *self.undo.lock().unwrap_or_else(|p| p.into_inner()))
    }

    pub(crate) fn take_staged(&self) -> Vec<StagedEffect> {
        std::mem::take(&mut *self.staged.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

/// A tool the proxy can execute on behalf of an agent.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique name used in `ExecuteStepRequest.tool_name`.
    fn name(&self) -> &str;

    /// Executes with resolved JSON arguments, returning a JSON output.
    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Value, ToolError>;
}

/// Thread-safe tool registry.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: Arc<DashMap<String, Arc<dyn Tool>>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry").field("tools", &self.names()).finish()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `tool`, returning any tool previously registered under its name.
    pub fn register(&self, tool: Arc<dyn Tool>) -> Option<Arc<dyn Tool>> {
        self.tools.insert(tool.name().to_owned(), tool)
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).map(|t| Arc::clone(t.value()))
    }

    /// Sorted tool names.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.iter().map(|e| e.key().clone()).collect();
        names.sort_unstable();
        names
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Runs tools in isolated tasks with a deadline.
#[derive(Debug, Clone)]
pub struct StepExecutor {
    timeout: Duration,
}

impl StepExecutor {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }

    /// Executes `tool` on its own task. Panics and timeouts become
    /// [`ToolError::Failed`] so they flow through the rollback engine.
    pub async fn run(
        &self,
        tool: Arc<dyn Tool>,
        ctx: Arc<ToolContext>,
        args: Value,
    ) -> Result<Value, ToolError> {
        let name = tool.name().to_owned();
        let mut handle = tokio::spawn(async move { tool.execute(&ctx, args).await });
        match tokio::time::timeout(self.timeout, &mut handle).await {
            Ok(Ok(result)) => result,
            Ok(Err(join)) if join.is_panic() => {
                let payload = join.into_panic();
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic payload".into());
                Err(ToolError::failed(format!("tool '{name}' panicked: {message}")))
            }
            Ok(Err(join)) => Err(ToolError::failed(format!("tool '{name}' was cancelled: {join}"))),
            Err(_) => {
                handle.abort();
                Err(ToolError::failed(format!(
                    "tool '{name}' timed out after {} ms",
                    self.timeout.as_millis()
                )))
            }
        }
    }
}

// ===========================================================================
// Reference resolution
// ===========================================================================

static REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\{steps\.(\d+)((?:\.[A-Za-z0-9_\-]+)*)\}").expect("static regex")
});

/// Arguments with every `${steps.N.path}` substituted.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedArguments {
    pub value: Value,
    pub dependencies: Vec<Dependency>,
}

/// A reference that could not be resolved. `dependencies` includes the
/// failing reference when its step exists, so root-cause analysis can jump
/// straight to the producer.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolutionFailure {
    pub message: String,
    pub dependencies: Vec<Dependency>,
}

/// Resolves `${steps.N.a.b.0}` references against outputs of completed steps.
///
/// * A string that is exactly one reference is replaced by the referenced
///   JSON value (type preserved).
/// * References embedded in longer strings are interpolated (strings
///   verbatim, other values as compact JSON).
/// * Numeric path segments index arrays.
pub fn resolve_references(
    args: &Value,
    current_step: u32,
    steps: &BTreeMap<u32, StepRecord>,
) -> Result<ResolvedArguments, ResolutionFailure> {
    let mut dependencies = Vec::new();
    match resolve_value(args, "", current_step, steps, &mut dependencies) {
        Ok(value) => Ok(ResolvedArguments {
            value,
            dependencies,
        }),
        Err(message) => Err(ResolutionFailure {
            message,
            dependencies,
        }),
    }
}

fn resolve_value(
    value: &Value,
    field: &str,
    current_step: u32,
    steps: &BTreeMap<u32, StepRecord>,
    deps: &mut Vec<Dependency>,
) -> Result<Value, String> {
    match value {
        Value::String(s) => resolve_string(s, field, current_step, steps, deps),
        Value::Array(items) => items
            .iter()
            .map(|item| resolve_value(item, field, current_step, steps, deps))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| resolve_value(v, k, current_step, steps, deps).map(|v| (k.clone(), v)))
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

fn resolve_string(
    s: &str,
    field: &str,
    current_step: u32,
    steps: &BTreeMap<u32, StepRecord>,
    deps: &mut Vec<Dependency>,
) -> Result<Value, String> {
    if !s.contains("${steps.") {
        return Ok(Value::String(s.to_owned()));
    }
    let mut interpolated = String::with_capacity(s.len());
    let mut last = 0;
    for caps in REFERENCE.captures_iter(s) {
        let whole = caps.get(0).expect("group 0 always present");
        let reference = whole.as_str();
        let unavailable =
            || format!("reference {reference} points to a step that has not completed");
        let step_id: u32 = caps[1].parse().map_err(|_| unavailable())?;
        let record = steps
            .get(&step_id)
            .filter(|_| step_id < current_step)
            .ok_or_else(unavailable)?;

        let segments: Vec<&str> = caps[2].split('.').filter(|p| !p.is_empty()).collect();
        let mut cursor = &record.output;
        for segment in &segments {
            let next = match cursor {
                Value::Object(map) => map.get(*segment),
                Value::Array(items) => segment.parse::<usize>().ok().and_then(|i| items.get(i)),
                _ => None,
            };
            cursor = match next {
                Some(v) => v,
                None => {
                    deps.push(Dependency {
                        from_step: step_id,
                        source_key: (*segment).to_owned(),
                        target_key: (*segment).to_owned(),
                        kind: DependencyKind::Explicit,
                    });
                    return Err(format!(
                        "unresolved reference {reference}: key '{segment}' not present in output of step {step_id}"
                    ));
                }
            };
        }

        let source_key = segments
            .iter()
            .rev()
            .find(|s| s.parse::<usize>().is_err())
            .map_or_else(|| field.to_owned(), |s| (*s).to_owned());
        let target_key = if field.is_empty() { source_key.clone() } else { field.to_owned() };
        deps.push(Dependency {
            from_step: step_id,
            source_key,
            target_key,
            kind: DependencyKind::Explicit,
        });

        if whole.start() == 0 && whole.end() == s.len() {
            return Ok(cursor.clone());
        }
        interpolated.push_str(&s[last..whole.start()]);
        match cursor {
            Value::String(text) => interpolated.push_str(text),
            other => interpolated.push_str(&other.to_string()),
        }
        last = whole.end();
    }
    interpolated.push_str(&s[last..]);
    Ok(Value::String(interpolated))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{RocksStore, StoreOptions};
    use serde_json::json;

    fn steps() -> BTreeMap<u32, StepRecord> {
        let mut map = BTreeMap::new();
        map.insert(
            1,
            StepRecord {
                step_id: 1,
                tool_name: "create_user".into(),
                arguments: json!({}),
                output: json!({ "id": 101, "profile": { "email": "a@b.c", "tags": ["x", "y"] } }),
            },
        );
        map
    }

    #[test]
    fn whole_reference_preserves_type_and_records_dependency() {
        let resolved =
            resolve_references(&json!({ "user_id": "${steps.1.id}" }), 2, &steps()).unwrap();
        assert_eq!(resolved.value, json!({ "user_id": 101 }));
        assert_eq!(
            resolved.dependencies,
            vec![Dependency {
                from_step: 1,
                source_key: "id".into(),
                target_key: "user_id".into(),
                kind: DependencyKind::Explicit,
            }]
        );
    }

    #[test]
    fn interpolates_nested_paths_and_array_indices() {
        let resolved = resolve_references(
            &json!({ "msg": "mail ${steps.1.profile.email} tag=${steps.1.profile.tags.1} id=${steps.1.id}", "n": [1, "${steps.1.profile.tags.0}"] }),
            2,
            &steps(),
        )
        .unwrap();
        assert_eq!(resolved.value["msg"], "mail a@b.c tag=y id=101");
        assert_eq!(resolved.value["n"], json!([1, "x"]));
        assert_eq!(resolved.dependencies.len(), 4);
        assert_eq!(resolved.dependencies[1].source_key, "tags");
    }

    #[test]
    fn missing_key_reports_producer_dependency() {
        let failure =
            resolve_references(&json!({ "user_id": "${steps.1.uid}" }), 2, &steps()).unwrap_err();
        assert_eq!(
            failure.message,
            "unresolved reference ${steps.1.uid}: key 'uid' not present in output of step 1"
        );
        assert_eq!(failure.dependencies[0].from_step, 1);
        assert_eq!(failure.dependencies[0].target_key, "uid");
    }

    #[test]
    fn future_or_unknown_steps_are_rejected() {
        for arg in ["${steps.2.id}", "${steps.1.id}", "${steps.99999999999.id}"] {
            let current = if arg == "${steps.1.id}" { 1 } else { 5 };
            let failure = resolve_references(&json!({ "a": arg }), current, &steps()).unwrap_err();
            assert!(failure.message.contains("has not completed"), "{arg}: {}", failure.message);
        }
    }

    #[test]
    fn strings_without_references_are_untouched() {
        let args = json!({ "a": "$ {not a ref}", "b": "${other.1}", "c": 3 });
        let resolved = resolve_references(&args, 2, &steps()).unwrap();
        assert_eq!(resolved.value, args);
        assert!(resolved.dependencies.is_empty());
    }

    struct Sleepy;
    #[async_trait]
    impl Tool for Sleepy {
        fn name(&self) -> &str {
            "sleepy"
        }
        async fn execute(&self, _ctx: &ToolContext, args: Value) -> Result<Value, ToolError> {
            if args["panic"] == true {
                panic!("kaboom");
            }
            tokio::time::sleep(Duration::from_millis(args["ms"].as_u64().unwrap_or(0))).await;
            Ok(json!({ "slept": true }))
        }
    }

    fn ctx() -> (tempfile::TempDir, Arc<ToolContext>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(RocksStore::open(dir.path(), StoreOptions::default()).unwrap());
        (dir, Arc::new(ToolContext::new(store.scoped("tx").unwrap(), 1)))
    }

    #[tokio::test]
    async fn executor_converts_timeouts_and_panics_into_failures() {
        let (_dir, ctx) = ctx();
        let exec = StepExecutor::new(Duration::from_millis(50));
        let tool: Arc<dyn Tool> = Arc::new(Sleepy);

        let ok = exec.run(Arc::clone(&tool), Arc::clone(&ctx), json!({ "ms": 1 })).await;
        assert_eq!(ok.unwrap(), json!({ "slept": true }));

        let timeout = exec.run(Arc::clone(&tool), Arc::clone(&ctx), json!({ "ms": 5_000 })).await;
        assert!(matches!(timeout, Err(ToolError::Failed(m)) if m.contains("timed out after 50 ms")));

        let panic = exec.run(tool, ctx, json!({ "panic": true })).await;
        assert!(matches!(panic, Err(ToolError::Failed(m)) if m.contains("panicked: kaboom")));
    }

    #[tokio::test]
    async fn context_collects_side_effect_declarations() {
        let (_dir, ctx) = ctx();
        let key = ctx.stage_effect("email", json!({ "to": "x" }), None);
        ctx.stage_effect("webhook", Value::Null, Some("fixed".into()));
        ctx.register_undo(Arc::new(crate::ledger::FnUndo::new("noop", || async { Ok(()) })));
        let staged = ctx.take_staged();
        assert_eq!(staged.len(), 2);
        assert_eq!(staged[0].idempotency_key, key);
        assert_eq!(staged[1].idempotency_key, "fixed");
        assert_eq!(ctx.take_undo().len(), 1);
        assert!(ctx.take_staged().is_empty());
    }

    #[test]
    fn registry_lists_sorted_names() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert!(registry.register(Arc::new(Sleepy)).is_none());
        assert!(registry.register(Arc::new(Sleepy)).is_some());
        assert_eq!(registry.names(), vec!["sleepy"]);
        assert!(registry.get("missing").is_none());
    }
}
