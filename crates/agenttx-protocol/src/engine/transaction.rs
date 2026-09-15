//! In-memory state of one agent transaction.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use serde_json::Value;

use super::state_machine::{RollbackController, RollbackPolicy, TxStatus};
use crate::errors::{AgentTxError, Result};
use crate::ledger::{DependencyGraph, SagaLedger, StagingQueue};
use crate::storage::snapshot::now_ms;

/// A successfully executed step.
#[derive(Debug, Clone, PartialEq)]
pub struct StepRecord {
    pub step_id: u32,
    pub tool_name: String,
    /// Arguments after `${steps.…}` resolution.
    pub arguments: Value,
    pub output: Value,
}

/// LLM context that produced a successful step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextEntry {
    pub step_id: u32,
    pub tool_name: String,
    pub raw_context: String,
}

/// Mutable transaction state, guarded by a per-transaction mutex in the engine.
#[derive(Debug)]
pub struct Transaction {
    pub id: String,
    pub agent_id: String,
    pub metadata: HashMap<String, String>,
    pub created_at_ms: u64,
    pub last_activity: Instant,
    pub status: TxStatus,
    /// Step id the agent must send next (steps are 1-based).
    pub next_step: u32,
    pub steps: BTreeMap<u32, StepRecord>,
    pub context: Vec<ContextEntry>,
    /// Aggregated, de-duplicated Clean Hints for the system prompt.
    pub constraints: Vec<String>,
    pub graph: DependencyGraph,
    pub controller: RollbackController,
    pub saga: SagaLedger,
    pub staging: StagingQueue,
}

impl Transaction {
    pub fn new(
        id: String,
        agent_id: String,
        metadata: HashMap<String, String>,
        policy: RollbackPolicy,
        saga: SagaLedger,
    ) -> Self {
        Self {
            id,
            agent_id,
            metadata,
            created_at_ms: now_ms(),
            last_activity: Instant::now(),
            status: TxStatus::Active,
            next_step: 1,
            steps: BTreeMap::new(),
            context: Vec::new(),
            constraints: Vec::new(),
            graph: DependencyGraph::new(),
            controller: RollbackController::new(policy),
            saga,
            staging: StagingQueue::new(),
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn ensure_active(&self) -> Result<()> {
        if self.status == TxStatus::Active {
            Ok(())
        } else {
            Err(AgentTxError::InvalidTransactionState {
                id: self.id.clone(),
                status: self.status.to_string(),
                expected: "ACTIVE",
            })
        }
    }

    pub fn transition(&mut self, next: TxStatus) -> Result<()> {
        self.status.transition(next)
    }

    /// Adds a constraint unless already present, evicting the oldest beyond
    /// `max`. Returns whether it was newly added.
    pub fn add_constraint(&mut self, hint: &str, max: usize) -> bool {
        if self.constraints.iter().any(|c| c == hint) {
            return false;
        }
        self.constraints.push(hint.to_owned());
        if self.constraints.len() > max.max(1) {
            let excess = self.constraints.len() - max.max(1);
            self.constraints.drain(..excess);
        }
        true
    }

    /// Drops in-memory records of `step_id` and all later steps.
    /// `step_id == 0` clears everything (global reset).
    pub fn truncate_from(&mut self, step_id: u32) {
        if step_id == 0 {
            self.steps.clear();
            self.context.clear();
            self.graph.clear();
        } else {
            self.steps.split_off(&step_id);
            self.context.retain(|c| c.step_id < step_id);
            self.graph.truncate_from(step_id);
        }
        self.next_step = step_id.max(1);
    }

    pub fn output_of(&self, step_id: u32) -> Option<&Value> {
        self.steps.get(&step_id).map(|s| &s.output)
    }

    /// Highest successfully executed step, if any.
    pub fn last_step(&self) -> Option<u32> {
        self.steps.keys().next_back().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{RocksStore, StoreOptions};
    use serde_json::json;
    use std::sync::Arc;

    fn tx() -> (tempfile::TempDir, Transaction) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(RocksStore::open(dir.path(), StoreOptions::default()).unwrap());
        let saga = SagaLedger::new("tx", store, 1);
        let tx = Transaction::new(
            "tx".into(),
            "agent".into(),
            HashMap::new(),
            RollbackPolicy::default(),
            saga,
        );
        (dir, tx)
    }

    fn push_step(tx: &mut Transaction, step_id: u32) {
        tx.graph.add_step(step_id, "t", &json!({}), &[]);
        tx.graph.record_output(step_id, &json!({ "v": step_id }));
        tx.steps.insert(
            step_id,
            StepRecord {
                step_id,
                tool_name: "t".into(),
                arguments: json!({}),
                output: json!({ "v": step_id }),
            },
        );
        tx.context.push(ContextEntry {
            step_id,
            tool_name: "t".into(),
            raw_context: format!("ctx {step_id}"),
        });
        tx.next_step = step_id + 1;
    }

    #[test]
    fn truncate_from_rewinds_all_views() {
        let (_dir, mut tx) = tx();
        for step in 1..=4 {
            push_step(&mut tx, step);
        }
        tx.truncate_from(3);
        assert_eq!(tx.next_step, 3);
        assert_eq!(tx.last_step(), Some(2));
        assert_eq!(tx.context.len(), 2);
        assert!(!tx.graph.contains(3));
        assert_eq!(tx.output_of(2), Some(&json!({ "v": 2 })));

        tx.truncate_from(0);
        assert_eq!(tx.next_step, 1);
        assert!(tx.steps.is_empty() && tx.context.is_empty() && tx.graph.is_empty());
    }

    #[test]
    fn constraints_are_deduplicated_and_bounded() {
        let (_dir, mut tx) = tx();
        assert!(tx.add_constraint("Hint: a", 2));
        assert!(!tx.add_constraint("Hint: a", 2));
        assert!(tx.add_constraint("Hint: b", 2));
        assert!(tx.add_constraint("Hint: c", 2));
        assert_eq!(tx.constraints, vec!["Hint: b", "Hint: c"]);
    }

    #[test]
    fn ensure_active_reflects_status() {
        let (_dir, mut tx) = tx();
        tx.ensure_active().unwrap();
        tx.transition(TxStatus::Committing).unwrap();
        assert!(matches!(
            tx.ensure_active(),
            Err(AgentTxError::InvalidTransactionState { .. })
        ));
    }
}
