//! Transaction lifecycle states and the hybrid cascading rollback controller.
//!
//! # Decision loop
//!
//! When step `K` fails (after the error has been reduced to a Clean Hint):
//!
//! 1. **Dependency Jump** — if the hint names a key and the
//!    [`DependencyGraph`] traces it to an earlier root-cause step `R < K`
//!    (jumped to fewer than `max(max_local_depth, 1)` times this epoch),
//!    rewind to just before `R`.
//! 2. **Local Backtrack** — else, on the `d`-th failure of `K` with
//!    `d <= max_local_depth`, rewind `d` steps to `max(K - d, 1)`
//!    (depth 1 re-runs `K-1` and `K`; depth 2 re-runs from `K-2`).
//! 3. **Global Step-0 Reset** — else rewind to the initial snapshot, clear the
//!    agent context and carry aggregated hints as constraints.
//!
//! A jump or local backtrack from `K` to `T` costs `K - T + 1` replayed steps.
//! If that would push cumulative replays past the budget
//! `c · N · ⌈log₂(N + 1)⌉` (N = highest step reached), the controller
//! escalates to a global reset instead, so backtracking cannot degrade into
//! O(N²) re-execution.
//!
//! # Termination
//!
//! Within an epoch (the span between global resets) every failure either adds
//! at least one step to the bounded replay counter or escalates. Escalations
//! are capped by `max_global_resets`, after which the transaction aborts.
//! Hence total re-executions are bounded by
//! `(max_global_resets + 1) · (budget + N)`: O(N log N) per epoch and O(N) per
//! reset.

use std::collections::HashMap;
use std::fmt;

use crate::errors::{AgentTxError, Result};
use crate::ledger::DependencyGraph;

// ===========================================================================
// Transaction status
// ===========================================================================

/// Lifecycle state of a transaction.
///
/// ```text
///            ┌──────────── Executing ◀──┐
///            ▼                │         │
///  Active ◀──┴── RollingBack ◀┘         │
///    │  ▲            │    │             │
///    │  └────────────┘    ├─▶ RolledBack│
///    │                    └─▶ Failed    │
///    ├─▶ Committing ─▶ Committed        │
///    │   (Committing ─▶ Active on a retryable storage error)
///    └──────────────────────────────────┘
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TxStatus {
    Active,
    Executing,
    RollingBack,
    Committing,
    Committed,
    RolledBack,
    Failed,
}

impl TxStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TxStatus::Active => "ACTIVE",
            TxStatus::Executing => "EXECUTING",
            TxStatus::RollingBack => "ROLLING_BACK",
            TxStatus::Committing => "COMMITTING",
            TxStatus::Committed => "COMMITTED",
            TxStatus::RolledBack => "ROLLED_BACK",
            TxStatus::Failed => "FAILED",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, TxStatus::Committed | TxStatus::RolledBack | TxStatus::Failed)
    }

    pub fn can_transition_to(self, next: TxStatus) -> bool {
        use TxStatus::*;
        matches!(
            (self, next),
            (Active, Executing | Committing | RollingBack)
                | (Executing, Active | RollingBack)
                | (RollingBack, Active | RolledBack | Failed)
                | (Committing, Committed | Active | RollingBack)
        )
    }

    /// Moves `self` to `next`, rejecting illegal transitions.
    pub fn transition(&mut self, next: TxStatus) -> Result<()> {
        if !self.can_transition_to(next) {
            return Err(AgentTxError::IllegalTransition {
                from: self.as_str().into(),
                to: next.as_str().into(),
            });
        }
        *self = next;
        Ok(())
    }
}

impl fmt::Display for TxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ===========================================================================
// Policy
// ===========================================================================

/// Bounds for the rollback controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RollbackPolicy {
    /// Maximum local backtrack depth D.
    pub max_local_depth: u32,
    /// Global resets allowed before the transaction aborts.
    pub max_global_resets: u32,
    /// Constant `c` of the replay budget `c · N · ⌈log₂(N + 1)⌉`.
    pub replay_budget_factor: f64,
}

impl RollbackPolicy {
    pub const DEFAULT_MAX_LOCAL_DEPTH: u32 = 2;
    pub const DEFAULT_MAX_GLOBAL_RESETS: u32 = 1;
    pub const DEFAULT_REPLAY_BUDGET_FACTOR: f64 = 2.0;
    /// Upper bound on D; deeper local rewinds should be global resets.
    pub const MAX_LOCAL_DEPTH_LIMIT: u32 = 16;
    pub const MAX_GLOBAL_RESETS_LIMIT: u32 = 16;

    pub fn validate(&self) -> Result<()> {
        if self.max_local_depth > Self::MAX_LOCAL_DEPTH_LIMIT {
            return Err(AgentTxError::InvalidArgument(format!(
                "max_local_depth must be <= {}",
                Self::MAX_LOCAL_DEPTH_LIMIT
            )));
        }
        if self.max_global_resets > Self::MAX_GLOBAL_RESETS_LIMIT {
            return Err(AgentTxError::InvalidArgument(format!(
                "max_global_resets must be <= {}",
                Self::MAX_GLOBAL_RESETS_LIMIT
            )));
        }
        if !self.replay_budget_factor.is_finite()
            || !(0.1..=1000.0).contains(&self.replay_budget_factor)
        {
            return Err(AgentTxError::InvalidArgument(
                "replay_budget_factor must be within [0.1, 1000]".into(),
            ));
        }
        Ok(())
    }

    /// Replayed-step budget for a transaction whose furthest step is `n`.
    pub fn replay_budget(&self, n: u32) -> u64 {
        let n = f64::from(n.max(1));
        let budget = self.replay_budget_factor * n * (n + 1.0).log2().ceil();
        (budget.ceil() as u64).max(1)
    }
}

impl Default for RollbackPolicy {
    fn default() -> Self {
        Self {
            max_local_depth: Self::DEFAULT_MAX_LOCAL_DEPTH,
            max_global_resets: Self::DEFAULT_MAX_GLOBAL_RESETS,
            replay_budget_factor: Self::DEFAULT_REPLAY_BUDGET_FACTOR,
        }
    }
}

// ===========================================================================
// Decisions
// ===========================================================================

/// Strategy applied in response to a failure or request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RollbackStrategy {
    None,
    LocalBacktrack,
    DependencyJump,
    GlobalReset,
    Manual,
    Abort,
}

impl RollbackStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            RollbackStrategy::None => "NONE",
            RollbackStrategy::LocalBacktrack => "LOCAL_BACKTRACK",
            RollbackStrategy::DependencyJump => "DEPENDENCY_JUMP",
            RollbackStrategy::GlobalReset => "GLOBAL_RESET",
            RollbackStrategy::Manual => "MANUAL",
            RollbackStrategy::Abort => "ABORT",
        }
    }
}

/// Why the controller escalated past local strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscalationReason {
    /// The failing step exhausted its local backtrack depth.
    LocalDepthExhausted,
    /// The rollback would exceed the O(N log N) replay budget.
    ReplayBudgetExceeded,
}

impl fmt::Display for EscalationReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EscalationReason::LocalDepthExhausted => "local backtrack depth exhausted",
            EscalationReason::ReplayBudgetExceeded => "replay budget exceeded",
        })
    }
}

/// Output of [`RollbackController::decide`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackDecision {
    /// Rewind to just before the root-cause step.
    DependencyJump { target_step: u32, root_key: String },
    /// Rewind `depth` steps.
    LocalBacktrack { target_step: u32, depth: u32 },
    /// Rewind to Step 0 and reset the agent context.
    GlobalReset { reason: EscalationReason },
    /// Stop: compensate everything and fail the transaction.
    Abort { reason: String },
}

impl RollbackDecision {
    pub fn strategy(&self) -> RollbackStrategy {
        match self {
            RollbackDecision::DependencyJump { .. } => RollbackStrategy::DependencyJump,
            RollbackDecision::LocalBacktrack { .. } => RollbackStrategy::LocalBacktrack,
            RollbackDecision::GlobalReset { .. } => RollbackStrategy::GlobalReset,
            RollbackDecision::Abort { .. } => RollbackStrategy::Abort,
        }
    }

    /// First step whose effects are undone (0 for reset/abort).
    pub fn target_step(&self) -> u32 {
        match self {
            RollbackDecision::DependencyJump { target_step, .. }
            | RollbackDecision::LocalBacktrack { target_step, .. } => *target_step,
            RollbackDecision::GlobalReset { .. } | RollbackDecision::Abort { .. } => 0,
        }
    }
}

// ===========================================================================
// Controller
// ===========================================================================

/// Per-transaction rollback bookkeeping. Pure: performs no I/O.
#[derive(Debug, Clone)]
pub struct RollbackController {
    policy: RollbackPolicy,
    /// Failures per step in the current epoch.
    retries: HashMap<u32, u32>,
    /// Jumps per root step in the current epoch.
    jumps: HashMap<u32, u32>,
    /// Steps replayed by jumps/backtracks in the current epoch.
    replayed_steps: u64,
    global_resets: u32,
    /// Furthest step observed (N), preserved across resets.
    horizon: u32,
}

impl RollbackController {
    pub fn new(policy: RollbackPolicy) -> Self {
        Self {
            policy,
            retries: HashMap::new(),
            jumps: HashMap::new(),
            replayed_steps: 0,
            global_resets: 0,
            horizon: 0,
        }
    }

    pub fn policy(&self) -> &RollbackPolicy {
        &self.policy
    }

    pub fn global_resets(&self) -> u32 {
        self.global_resets
    }

    pub fn replayed_steps(&self) -> u64 {
        self.replayed_steps
    }

    pub fn retries_for(&self, step_id: u32) -> u32 {
        self.retries.get(&step_id).copied().unwrap_or(0)
    }

    /// Records that `step_id` executed (successfully or not).
    pub fn observe_step(&mut self, step_id: u32) {
        self.horizon = self.horizon.max(step_id);
    }

    /// Current replay budget, based on the furthest step reached.
    pub fn replay_budget(&self) -> u64 {
        self.policy.replay_budget(self.horizon)
    }

    /// Chooses how to recover from the failure of `failed_step`.
    ///
    /// `error_key` is the offending key extracted from the Clean Hint.
    pub fn decide(
        &mut self,
        failed_step: u32,
        error_key: Option<&str>,
        graph: &DependencyGraph,
    ) -> RollbackDecision {
        let failed_step = failed_step.max(1);
        self.observe_step(failed_step);
        let attempt = {
            let count = self.retries.entry(failed_step).or_insert(0);
            *count += 1;
            *count
        };

        let candidate = self
            .jump_candidate(failed_step, error_key, graph)
            .or_else(|| {
                (attempt <= self.policy.max_local_depth).then(|| RollbackDecision::LocalBacktrack {
                    target_step: failed_step.saturating_sub(attempt).max(1),
                    depth: attempt,
                })
            });
        let Some(decision) = candidate else {
            return self.escalate(EscalationReason::LocalDepthExhausted);
        };

        let cost = u64::from(failed_step - decision.target_step() + 1);
        if self.replayed_steps + cost > self.replay_budget() {
            return self.escalate(EscalationReason::ReplayBudgetExceeded);
        }
        self.replayed_steps += cost;
        if let RollbackDecision::DependencyJump { target_step, .. } = &decision {
            *self.jumps.entry(*target_step).or_insert(0) += 1;
        }
        decision
    }

    fn jump_candidate(
        &self,
        failed_step: u32,
        error_key: Option<&str>,
        graph: &DependencyGraph,
    ) -> Option<RollbackDecision> {
        let key = error_key?;
        let root = graph
            .find_root_cause(failed_step, key)
            .filter(|root| *root < failed_step)?;
        let max_jumps = self.policy.max_local_depth.max(1);
        (self.jumps.get(&root).copied().unwrap_or(0) < max_jumps).then(|| {
            RollbackDecision::DependencyJump {
                target_step: root,
                root_key: key.to_owned(),
            }
        })
    }

    fn escalate(&mut self, reason: EscalationReason) -> RollbackDecision {
        if self.global_resets >= self.policy.max_global_resets {
            return RollbackDecision::Abort {
                reason: format!(
                    "{reason}; global reset budget ({}) exhausted",
                    self.policy.max_global_resets
                ),
            };
        }
        self.global_resets += 1;
        self.retries.clear();
        self.jumps.clear();
        self.replayed_steps = 0;
        RollbackDecision::GlobalReset { reason }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn policy(depth: u32, resets: u32, factor: f64) -> RollbackPolicy {
        RollbackPolicy {
            max_local_depth: depth,
            max_global_resets: resets,
            replay_budget_factor: factor,
        }
    }

    /// Steps 1..=n with no data dependencies.
    fn independent_graph(n: u32) -> DependencyGraph {
        let mut g = DependencyGraph::new();
        for step in 1..=n {
            g.add_step(step, "t", &json!({ "n": format!("in-{step}") }), &[]);
            g.record_output(step, &json!({ "out": format!("out-{step}") }));
        }
        g
    }

    #[test]
    fn status_transitions() {
        let mut s = TxStatus::Active;
        s.transition(TxStatus::Executing).unwrap();
        s.transition(TxStatus::RollingBack).unwrap();
        s.transition(TxStatus::Active).unwrap();
        s.transition(TxStatus::Committing).unwrap();
        s.transition(TxStatus::Committed).unwrap();
        assert!(s.is_terminal());
        assert!(s.transition(TxStatus::Active).is_err());

        let mut s = TxStatus::Active;
        assert!(s.transition(TxStatus::Committed).is_err());
        assert_eq!(s, TxStatus::Active, "failed transition leaves state unchanged");
        assert!(!TxStatus::Failed.can_transition_to(TxStatus::Active));
        assert!(TxStatus::Committing.can_transition_to(TxStatus::RollingBack));
    }

    #[test]
    fn policy_validation_and_budget() {
        RollbackPolicy::default().validate().unwrap();
        assert!(policy(99, 1, 2.0).validate().is_err());
        assert!(policy(2, 99, 2.0).validate().is_err());
        assert!(policy(2, 1, f64::NAN).validate().is_err());
        assert!(policy(2, 1, 0.0).validate().is_err());

        let p = RollbackPolicy::default();
        assert_eq!(p.replay_budget(0), 2); // N clamps to 1: 2·1·1
        assert_eq!(p.replay_budget(3), 12); // 2·3·2
        assert_eq!(p.replay_budget(10), 80); // 2·10·4
        assert!(p.replay_budget(1000) < 2 * 1000 * 1000);
    }

    #[test]
    fn local_backtrack_deepens_then_escalates_to_global_reset() {
        let graph = independent_graph(5);
        let mut c = RollbackController::new(policy(2, 1, 100.0));

        assert_eq!(
            c.decide(5, None, &graph),
            RollbackDecision::LocalBacktrack { target_step: 4, depth: 1 }
        );
        assert_eq!(
            c.decide(5, None, &graph),
            RollbackDecision::LocalBacktrack { target_step: 3, depth: 2 }
        );
        assert_eq!(c.retries_for(5), 2);
        assert_eq!(c.replayed_steps(), 2 + 3);

        assert_eq!(
            c.decide(5, None, &graph),
            RollbackDecision::GlobalReset { reason: EscalationReason::LocalDepthExhausted }
        );
        assert_eq!(c.global_resets(), 1);
        assert_eq!(c.retries_for(5), 0, "reset starts a new epoch");
        assert_eq!(c.replayed_steps(), 0);
    }

    #[test]
    fn aborts_when_global_resets_exhausted() {
        let graph = independent_graph(3);
        let mut c = RollbackController::new(policy(0, 1, 100.0));
        assert!(matches!(c.decide(3, None, &graph), RollbackDecision::GlobalReset { .. }));
        match c.decide(3, None, &graph) {
            RollbackDecision::Abort { reason } => {
                assert!(reason.contains("global reset budget (1) exhausted"), "{reason}");
            }
            other => panic!("expected abort, got {other:?}"),
        }
    }

    #[test]
    fn local_backtrack_never_targets_step_zero() {
        let graph = independent_graph(1);
        let mut c = RollbackController::new(policy(2, 0, 100.0));
        assert_eq!(
            c.decide(1, None, &graph),
            RollbackDecision::LocalBacktrack { target_step: 1, depth: 1 }
        );
        assert_eq!(
            c.decide(1, None, &graph),
            RollbackDecision::LocalBacktrack { target_step: 1, depth: 2 }
        );
    }

    #[test]
    fn dependency_jump_targets_root_cause() {
        let mut g = DependencyGraph::new();
        g.add_step(1, "create_user", &json!({}), &[]);
        g.record_output(1, &json!({ "id": "u-101" }));
        g.add_step(2, "noop", &json!({ "x": "a" }), &[]);
        g.record_output(2, &json!({ "y": "b" }));
        g.add_step(3, "noop", &json!({ "x": "c" }), &[]);
        g.record_output(3, &json!({ "y": "d" }));
        g.add_step(4, "insert_order", &json!({ "user_id": "u-101" }), &[]);

        let mut c = RollbackController::new(policy(2, 1, 100.0));
        assert_eq!(
            c.decide(4, Some("user_id"), &g),
            RollbackDecision::DependencyJump { target_step: 1, root_key: "user_id".into() }
        );
        // Unknown key falls back to local backtracking.
        assert_eq!(
            c.decide(4, Some("sku"), &g),
            RollbackDecision::LocalBacktrack { target_step: 2, depth: 2 }
        );
    }

    #[test]
    fn repeated_jumps_to_same_root_are_capped() {
        let mut g = DependencyGraph::new();
        g.add_step(1, "a", &json!({}), &[]);
        g.record_output(1, &json!({ "id": 7 }));
        g.add_step(2, "b", &json!({ "owner_id": 7 }), &[]);

        let mut c = RollbackController::new(policy(2, 1, 100.0));
        for _ in 0..2 {
            assert!(matches!(
                c.decide(2, Some("owner_id"), &g),
                RollbackDecision::DependencyJump { target_step: 1, .. }
            ));
        }
        // Jump cap (2) reached and step 2 already failed twice: escalate.
        assert!(matches!(
            c.decide(2, Some("owner_id"), &g),
            RollbackDecision::GlobalReset { reason: EscalationReason::LocalDepthExhausted }
        ));
    }

    #[test]
    fn replay_budget_forces_global_reset_before_quadratic_replay() {
        // N = 8, c = 0.5 → budget = ceil(0.5·8·4) = 16 replayed steps.
        let mut g = DependencyGraph::new();
        g.add_step(1, "root", &json!({}), &[]);
        g.record_output(1, &json!({ "token": "abc" }));
        for step in 2..=8 {
            g.add_step(step, "use", &json!({ "token": "abc" }), &[]);
            g.record_output(step, &json!({ "token": "abc" }));
        }
        let mut c = RollbackController::new(policy(2, 1, 0.5));
        c.observe_step(8);
        assert_eq!(c.replay_budget(), 16);

        // Jump 8 → 1 costs 8; a second costs 8 more (total 16, still within).
        for _ in 0..2 {
            assert!(matches!(
                c.decide(8, Some("token"), &g),
                RollbackDecision::DependencyJump { target_step: 1, .. }
            ));
        }
        assert_eq!(c.replayed_steps(), 16);
        // Any further rewind would exceed the budget.
        assert_eq!(
            c.decide(7, None, &g),
            RollbackDecision::GlobalReset { reason: EscalationReason::ReplayBudgetExceeded }
        );
    }

    #[test]
    fn total_decisions_are_bounded() {
        // Adversarial agent: step N fails forever. The controller must abort
        // after a bounded number of decisions.
        let n = 20;
        let graph = independent_graph(n);
        let p = RollbackPolicy::default();
        let mut c = RollbackController::new(p);
        let mut decisions = 0;
        loop {
            decisions += 1;
            if let RollbackDecision::Abort { .. } = c.decide(n, None, &graph) {
                break;
            }
            assert!(decisions < 1_000, "controller failed to terminate");
        }
        assert_eq!(c.global_resets(), p.max_global_resets);
    }

    #[test]
    fn decision_accessors() {
        let jump = RollbackDecision::DependencyJump { target_step: 3, root_key: "k".into() };
        assert_eq!(jump.strategy(), RollbackStrategy::DependencyJump);
        assert_eq!(jump.target_step(), 3);
        let reset = RollbackDecision::GlobalReset { reason: EscalationReason::ReplayBudgetExceeded };
        assert_eq!(reset.target_step(), 0);
        assert_eq!(reset.strategy().as_str(), "GLOBAL_RESET");
    }
}
