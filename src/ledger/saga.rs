//! Saga compensation ledger and staging queue for non-reversible effects.
//!
//! * **Reversible** external effects (files, rows in foreign systems, API
//!   resources) register an [`UndoAction`] when performed. Undo records are
//!   persisted to the `saga_logs` column family before the action is tracked
//!   in memory, so compensations can be rebuilt by an [`UndoRegistry`] after a
//!   crash. On rollback they run in reverse order (N → 1).
//! * **Non-reversible** effects (email, webhooks) are never executed during
//!   the transaction. They are placed on a [`StagingQueue`] and dispatched only
//!   after a successful `CommitTransaction`; rewinds simply discard them.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::errors::{AgentTxError, Result};
use crate::storage::RocksStore;
use crate::storage::snapshot::now_ms;

// ===========================================================================
// Undo actions
// ===========================================================================

/// A compensating action for one reversible side effect.
#[async_trait]
pub trait UndoAction: Send + Sync + fmt::Debug {
    /// Stable identifier used to rebuild the action from its payload after a
    /// crash (see [`UndoRegistry`]).
    fn kind(&self) -> &str;

    /// Human-readable description for logs and reports.
    fn describe(&self) -> String;

    /// Durable parameters sufficient to rebuild the action.
    fn payload(&self) -> Value;

    /// Performs the compensation. Must be idempotent: it may be retried.
    async fn undo(&self) -> std::result::Result<(), String>;
}

type UndoFuture = Pin<Box<dyn Future<Output = std::result::Result<(), String>> + Send>>;

/// Closure-backed undo action for in-process effects.
///
/// Closures cannot be persisted, so an `FnUndo` is not recoverable after a
/// crash; prefer a registered [`UndoAction`] type for durable effects.
pub struct FnUndo {
    description: String,
    f: Box<dyn Fn() -> UndoFuture + Send + Sync>,
}

impl FnUndo {
    pub const KIND: &'static str = "fn";

    pub fn new<F, Fut>(description: impl Into<String>, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = std::result::Result<(), String>> + Send + 'static,
    {
        Self {
            description: description.into(),
            f: Box::new(move || Box::pin(f())),
        }
    }
}

impl fmt::Debug for FnUndo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FnUndo")
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl UndoAction for FnUndo {
    fn kind(&self) -> &str {
        Self::KIND
    }

    fn describe(&self) -> String {
        self.description.clone()
    }

    fn payload(&self) -> Value {
        Value::Null
    }

    async fn undo(&self) -> std::result::Result<(), String> {
        (self.f)().await
    }
}

/// Factory rebuilding an undo action from its persisted payload.
pub type UndoFactory = Arc<dyn Fn(&Value) -> Result<Arc<dyn UndoAction>> + Send + Sync>;

/// Maps [`UndoAction::kind`] to factories for crash recovery.
#[derive(Clone, Default)]
pub struct UndoRegistry {
    factories: Arc<DashMap<String, UndoFactory>>,
}

impl fmt::Debug for UndoRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kinds: Vec<String> = self.factories.iter().map(|e| e.key().clone()).collect();
        f.debug_struct("UndoRegistry").field("kinds", &kinds).finish()
    }
}

impl UndoRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<F>(&self, kind: impl Into<String>, factory: F)
    where
        F: Fn(&Value) -> Result<Arc<dyn UndoAction>> + Send + Sync + 'static,
    {
        self.factories.insert(kind.into(), Arc::new(factory));
    }

    pub fn rebuild(&self, record: &SagaRecord) -> Result<Arc<dyn UndoAction>> {
        let factory = self
            .factories
            .get(&record.kind)
            .map(|f| Arc::clone(f.value()))
            .ok_or_else(|| AgentTxError::Compensation {
                action: record.description.clone(),
                reason: format!("no undo factory registered for kind `{}`", record.kind),
            })?;
        factory(&record.payload)
    }
}

/// Persisted form of a registered undo action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SagaRecord {
    pub tx_id: String,
    pub step_id: u32,
    pub seq: u64,
    pub kind: String,
    pub description: String,
    pub payload: Value,
    pub recorded_at_ms: u64,
}

/// Outcome of running compensations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompensationReport {
    /// Undo actions that completed successfully.
    pub succeeded: u32,
    /// `description: reason` for every action that failed all attempts.
    pub failures: Vec<String>,
}

impl CompensationReport {
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    fn merge(&mut self, other: CompensationReport) {
        self.succeeded += other.succeeded;
        self.failures.extend(other.failures);
    }
}

struct SagaEntry {
    record: SagaRecord,
    action: Arc<dyn UndoAction>,
}

/// Per-transaction ordered log of compensating actions.
pub struct SagaLedger {
    tx_id: String,
    store: Arc<RocksStore>,
    entries: Mutex<Vec<SagaEntry>>,
    next_seq: AtomicU64,
    max_attempts: u32,
}

impl fmt::Debug for SagaLedger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SagaLedger")
            .field("tx_id", &self.tx_id)
            .field("next_seq", &self.next_seq.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl SagaLedger {
    pub fn new(tx_id: impl Into<String>, store: Arc<RocksStore>, max_attempts: u32) -> Self {
        Self {
            tx_id: tx_id.into(),
            store,
            entries: Mutex::new(Vec::new()),
            next_seq: AtomicU64::new(0),
            max_attempts: max_attempts.max(1),
        }
    }

    /// Rebuilds a ledger from persisted records (crash recovery).
    ///
    /// Records whose kind has no registered factory are reported in the
    /// returned error list and left on disk for operator inspection.
    pub fn recover(
        tx_id: &str,
        store: Arc<RocksStore>,
        registry: &UndoRegistry,
        max_attempts: u32,
    ) -> Result<(Self, Vec<String>)> {
        let ledger = Self::new(tx_id, Arc::clone(&store), max_attempts);
        let mut entries = Vec::new();
        let mut errors = Vec::new();
        let mut max_seq = None;
        for bytes in store.saga_records(tx_id)? {
            let record: SagaRecord = serde_json::from_slice(&bytes)?;
            max_seq = max_seq.max(Some(record.seq));
            match registry.rebuild(&record) {
                Ok(action) => entries.push(SagaEntry { record, action }),
                Err(err) => errors.push(err.to_string()),
            }
        }
        entries.sort_by_key(|e| (e.record.step_id, e.record.seq));
        ledger
            .next_seq
            .store(max_seq.map_or(0, |s| s + 1), Ordering::SeqCst);
        // Constructed but not yet shared: no contention on the mutex.
        *ledger.entries.try_lock().map_err(|_| AgentTxError::Compensation {
            action: "recover".into(),
            reason: "ledger unexpectedly locked".into(),
        })? = entries;
        Ok((ledger, errors))
    }

    pub fn tx_id(&self) -> &str {
        &self.tx_id
    }

    /// Durably registers `action` as the compensation for an effect of `step_id`.
    pub async fn record(&self, step_id: u32, action: Arc<dyn UndoAction>) -> Result<()> {
        let seq = self.next_seq.fetch_add(1, Ordering::SeqCst);
        let record = SagaRecord {
            tx_id: self.tx_id.clone(),
            step_id,
            seq,
            kind: action.kind().to_owned(),
            description: action.describe(),
            payload: action.payload(),
            recorded_at_ms: now_ms(),
        };
        self.store
            .put_saga_record(&self.tx_id, step_id, seq, &serde_json::to_vec(&record)?)?;
        self.entries.lock().await.push(SagaEntry { record, action });
        Ok(())
    }

    pub async fn len(&self) -> usize {
        self.entries.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.entries.lock().await.is_empty()
    }

    /// Records for steps `>= step_id`, in execution order.
    pub async fn records_from(&self, step_id: u32) -> Vec<SagaRecord> {
        self.entries
            .lock()
            .await
            .iter()
            .filter(|e| e.record.step_id >= step_id)
            .map(|e| e.record.clone())
            .collect()
    }

    /// Runs, newest first, the compensations of every step `>= step_id`.
    ///
    /// Each action is attempted up to `max_attempts` times with exponential
    /// backoff. Failures do not stop the remaining compensations; failed
    /// records stay persisted for operator follow-up.
    pub async fn compensate_from(&self, step_id: u32) -> CompensationReport {
        let to_run: Vec<SagaEntry> = {
            let mut entries = self.entries.lock().await;
            let split = entries.partition_point(|e| e.record.step_id < step_id);
            entries.split_off(split)
        };
        let mut report = CompensationReport::default();
        for entry in to_run.into_iter().rev() {
            report.merge(self.run_one(entry).await);
        }
        report
    }

    /// Compensates every recorded effect (N → 1).
    pub async fn compensate_all(&self) -> CompensationReport {
        self.compensate_from(0).await
    }

    /// Drops all in-memory entries without compensating (after commit, whose
    /// storage batch already removed the persisted records).
    pub async fn forget_all(&self) {
        self.entries.lock().await.clear();
    }

    async fn run_one(&self, entry: SagaEntry) -> CompensationReport {
        let mut report = CompensationReport::default();
        let description = entry.action.describe();
        let mut last_error = String::new();
        for attempt in 0..self.max_attempts {
            match entry.action.undo().await {
                Ok(()) => {
                    if let Err(err) = self.store.delete_saga_record(
                        &self.tx_id,
                        entry.record.step_id,
                        entry.record.seq,
                    ) {
                        tracing::warn!(tx = %self.tx_id, %err, "failed to delete compensated saga record");
                    }
                    tracing::debug!(tx = %self.tx_id, step = entry.record.step_id, action = %description, "compensated");
                    report.succeeded += 1;
                    return report;
                }
                Err(err) => {
                    last_error = err;
                    if attempt + 1 < self.max_attempts {
                        tokio::time::sleep(Duration::from_millis(10u64 << attempt.min(8))).await;
                    }
                }
            }
        }
        tracing::error!(tx = %self.tx_id, step = entry.record.step_id, action = %description, error = %last_error, "compensation failed");
        report.failures.push(format!("{description}: {last_error}"));
        report
    }
}

// ===========================================================================
// Staging queue for non-reversible effects
// ===========================================================================

/// A non-reversible side effect awaiting commit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagedEffect {
    pub step_id: u32,
    /// Delivery channel, e.g. `email`, `webhook`.
    pub channel: String,
    pub payload: Value,
    /// Lets downstream relays deduplicate redeliveries.
    pub idempotency_key: String,
}

/// Delivers committed effects.
#[async_trait]
pub trait EffectDispatcher: Send + Sync {
    async fn dispatch(&self, tx_id: &str, effect: &StagedEffect) -> std::result::Result<(), String>;
}

/// Outcome of flushing a staging queue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushReport {
    pub dispatched: u32,
    pub errors: Vec<String>,
}

/// Thread-safe FIFO of staged effects; flushed only on commit.
#[derive(Debug, Default)]
pub struct StagingQueue {
    inner: std::sync::Mutex<VecDeque<StagedEffect>>,
}

impl StagingQueue {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<StagedEffect>> {
        // Critical sections never panic, but recover from poisoning anyway.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn stage(&self, effect: StagedEffect) {
        self.lock().push_back(effect);
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    pub fn pending(&self) -> Vec<StagedEffect> {
        self.lock().iter().cloned().collect()
    }

    /// Discards effects staged by steps `>= step_id`; returns how many.
    pub fn discard_from(&self, step_id: u32) -> usize {
        let mut queue = self.lock();
        let before = queue.len();
        queue.retain(|e| e.step_id < step_id);
        before - queue.len()
    }

    pub fn discard_all(&self) -> usize {
        let mut queue = self.lock();
        let n = queue.len();
        queue.clear();
        n
    }

    /// Drains the queue and dispatches effects in staging order.
    ///
    /// Effects are removed before dispatch (at-most-once from the queue's
    /// perspective); failures are reported, not retried.
    pub async fn flush(&self, tx_id: &str, dispatcher: &dyn EffectDispatcher) -> FlushReport {
        let drained: Vec<StagedEffect> = self.lock().drain(..).collect();
        let mut report = FlushReport::default();
        for effect in &drained {
            match dispatcher.dispatch(tx_id, effect).await {
                Ok(()) => report.dispatched += 1,
                Err(err) => report.errors.push(format!(
                    "{} effect {} (step {}): {err}",
                    effect.channel, effect.idempotency_key, effect.step_id
                )),
            }
        }
        report
    }
}

/// Transactional-outbox dispatcher: appends committed effects as JSON lines
/// (fsync'd) for an external relay to deliver to SMTP, webhooks, queues, ...
#[derive(Debug)]
pub struct OutboxDispatcher {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl OutboxDispatcher {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

#[derive(Serialize)]
struct OutboxLine<'a> {
    tx_id: &'a str,
    committed_at_ms: u64,
    #[serde(flatten)]
    effect: &'a StagedEffect,
}

#[async_trait]
impl EffectDispatcher for OutboxDispatcher {
    async fn dispatch(&self, tx_id: &str, effect: &StagedEffect) -> std::result::Result<(), String> {
        let mut line = serde_json::to_vec(&OutboxLine {
            tx_id,
            committed_at_ms: now_ms(),
            effect,
        })
        .map_err(|e| e.to_string())?;
        line.push(b'\n');

        let _guard = self.write_lock.lock().await;
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|e| format!("open outbox {}: {e}", self.path.display()))?;
        file.write_all(&line).await.map_err(|e| e.to_string())?;
        file.sync_data().await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::StoreOptions;
    use std::sync::atomic::AtomicU32;

    fn store() -> (tempfile::TempDir, Arc<RocksStore>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(RocksStore::open(dir.path(), StoreOptions::default()).unwrap());
        (dir, store)
    }

    fn recording_undo(log: &Arc<std::sync::Mutex<Vec<String>>>, name: &str) -> Arc<dyn UndoAction> {
        let log = Arc::clone(log);
        let name = name.to_owned();
        Arc::new(FnUndo::new(format!("undo {name}"), move || {
            let log = Arc::clone(&log);
            let name = name.clone();
            async move {
                log.lock().unwrap().push(name);
                Ok(())
            }
        }))
    }

    #[tokio::test]
    async fn compensates_in_reverse_order_from_step() {
        let (_dir, store) = store();
        let ledger = SagaLedger::new("tx", Arc::clone(&store), 1);
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        ledger.record(1, recording_undo(&log, "a")).await.unwrap();
        ledger.record(2, recording_undo(&log, "b")).await.unwrap();
        ledger.record(2, recording_undo(&log, "c")).await.unwrap();
        ledger.record(3, recording_undo(&log, "d")).await.unwrap();
        assert_eq!(store.saga_records("tx").unwrap().len(), 4);

        let report = ledger.compensate_from(2).await;
        assert_eq!(report.succeeded, 3);
        assert!(report.is_clean());
        assert_eq!(*log.lock().unwrap(), vec!["d", "c", "b"]);
        assert_eq!(ledger.len().await, 1);
        assert_eq!(store.saga_records("tx").unwrap().len(), 1);

        ledger.compensate_all().await;
        assert_eq!(*log.lock().unwrap(), vec!["d", "c", "b", "a"]);
        assert!(ledger.is_empty().await);
    }

    #[tokio::test]
    async fn retries_then_reports_failures_without_stopping() {
        let (_dir, store) = store();
        let ledger = SagaLedger::new("tx", Arc::clone(&store), 3);
        let attempts = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&attempts);
        ledger
            .record(
                1,
                Arc::new(FnUndo::new("flaky", move || {
                    let counter = Arc::clone(&counter);
                    async move {
                        counter.fetch_add(1, Ordering::SeqCst);
                        Err("remote unavailable".to_string())
                    }
                })),
            )
            .await
            .unwrap();
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        ledger.record(2, recording_undo(&log, "ok")).await.unwrap();

        let report = ledger.compensate_all().await;
        assert_eq!(report.succeeded, 1);
        assert_eq!(report.failures, vec!["flaky: remote unavailable".to_string()]);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        // Failed record stays persisted.
        assert_eq!(store.saga_records("tx").unwrap().len(), 1);
    }

    #[derive(Debug)]
    struct CounterUndo(Arc<AtomicU32>, u32);

    #[async_trait]
    impl UndoAction for CounterUndo {
        fn kind(&self) -> &str {
            "counter"
        }
        fn describe(&self) -> String {
            format!("counter -{}", self.1)
        }
        fn payload(&self) -> Value {
            serde_json::json!({ "amount": self.1 })
        }
        async fn undo(&self) -> std::result::Result<(), String> {
            self.0.fetch_add(self.1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn recovers_persisted_actions_through_registry() {
        let (_dir, store) = store();
        let total = Arc::new(AtomicU32::new(0));
        {
            let ledger = SagaLedger::new("tx", Arc::clone(&store), 1);
            ledger.record(1, Arc::new(CounterUndo(Arc::clone(&total), 2))).await.unwrap();
            ledger.record(2, Arc::new(CounterUndo(Arc::clone(&total), 5))).await.unwrap();
            ledger.record(2, Arc::new(FnUndo::new("volatile", || async { Ok(()) }))).await.unwrap();
        }

        let registry = UndoRegistry::new();
        let sink = Arc::clone(&total);
        registry.register("counter", move |payload| {
            let amount = payload["amount"].as_u64().unwrap_or(0) as u32;
            Ok(Arc::new(CounterUndo(Arc::clone(&sink), amount)) as Arc<dyn UndoAction>)
        });

        let (ledger, errors) = SagaLedger::recover("tx", Arc::clone(&store), &registry, 1).unwrap();
        assert_eq!(errors.len(), 1, "fn undo is not recoverable: {errors:?}");
        assert_eq!(ledger.len().await, 2);
        let report = ledger.compensate_all().await;
        assert_eq!(report.succeeded, 2);
        assert_eq!(total.load(Ordering::SeqCst), 7);
        // New records continue the sequence rather than overwriting.
        ledger.record(3, Arc::new(CounterUndo(Arc::clone(&total), 1))).await.unwrap();
        assert_eq!(ledger.records_from(3).await[0].seq, 3);
    }

    struct CollectingDispatcher(std::sync::Mutex<Vec<String>>);

    #[async_trait]
    impl EffectDispatcher for CollectingDispatcher {
        async fn dispatch(&self, _tx: &str, e: &StagedEffect) -> std::result::Result<(), String> {
            if e.channel == "broken" {
                return Err("smtp down".into());
            }
            self.0.lock().unwrap().push(e.idempotency_key.clone());
            Ok(())
        }
    }

    fn effect(step_id: u32, channel: &str, key: &str) -> StagedEffect {
        StagedEffect {
            step_id,
            channel: channel.into(),
            payload: Value::Null,
            idempotency_key: key.into(),
        }
    }

    #[tokio::test]
    async fn staging_queue_discards_by_step_and_flushes_in_order() {
        let queue = StagingQueue::new();
        queue.stage(effect(1, "email", "e1"));
        queue.stage(effect(2, "webhook", "w2"));
        queue.stage(effect(3, "email", "e3"));
        assert_eq!(queue.discard_from(3), 1);
        queue.stage(effect(3, "broken", "b3"));

        let dispatcher = CollectingDispatcher(std::sync::Mutex::new(Vec::new()));
        let report = queue.flush("tx", &dispatcher).await;
        assert_eq!(report.dispatched, 2);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(*dispatcher.0.lock().unwrap(), vec!["e1", "w2"]);
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn outbox_appends_json_lines() {
        let dir = tempfile::tempdir().unwrap();
        let outbox = OutboxDispatcher::new(dir.path().join("nested/outbox.jsonl"));
        outbox.dispatch("tx1", &effect(1, "email", "a")).await.unwrap();
        outbox.dispatch("tx1", &effect(2, "webhook", "b")).await.unwrap();
        let text = std::fs::read_to_string(outbox.path()).unwrap();
        let lines: Vec<Value> = text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["tx_id"], "tx1");
        assert_eq!(lines[1]["channel"], "webhook");
    }
}
