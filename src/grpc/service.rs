//! `AgentTxService` implementation: request validation and proto mapping.

use std::sync::Arc;

use tonic::{Request, Response, Status};

use super::proto::{
    self, BeginTransactionRequest, BeginTransactionResponse, CommitTransactionRequest,
    CommitTransactionResponse, ExecuteStepRequest, ExecuteStepResponse, GetTransactionRequest,
    GetTransactionResponse, RollbackTransactionRequest, RollbackTransactionResponse,
    agent_tx_service_server::AgentTxService,
};
use crate::engine::{Engine, RollbackPolicy, RollbackStrategy, StepOutcome, StepRequest, StepStatus};

/// Maximum accepted `arguments_json` size.
pub const MAX_ARGUMENTS_BYTES: usize = 1024 * 1024;
/// Maximum accepted `raw_context` size.
pub const MAX_CONTEXT_BYTES: usize = 1024 * 1024;
const MAX_TOOL_NAME_BYTES: usize = 128;
const MAX_METADATA_ENTRIES: usize = 64;

/// gRPC front-end over an [`Engine`].
#[derive(Debug, Clone)]
pub struct AgentTxGrpcService {
    engine: Arc<Engine>,
}

impl AgentTxGrpcService {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self { engine }
    }
}

fn require_tx_id(tx_id: &str) -> Result<(), Status> {
    if tx_id.is_empty() {
        Err(Status::invalid_argument("transaction_id is required"))
    } else {
        Ok(())
    }
}

fn policy_from_proto(p: proto::RollbackPolicy, defaults: RollbackPolicy) -> RollbackPolicy {
    RollbackPolicy {
        max_local_depth: p.max_local_depth.unwrap_or(defaults.max_local_depth),
        max_global_resets: p.max_global_resets.unwrap_or(defaults.max_global_resets),
        replay_budget_factor: p.replay_budget_factor.unwrap_or(defaults.replay_budget_factor),
    }
}

fn strategy_to_proto(strategy: RollbackStrategy) -> proto::RollbackStrategy {
    match strategy {
        RollbackStrategy::None => proto::RollbackStrategy::None,
        RollbackStrategy::LocalBacktrack => proto::RollbackStrategy::LocalBacktrack,
        RollbackStrategy::DependencyJump => proto::RollbackStrategy::DependencyJump,
        RollbackStrategy::GlobalReset => proto::RollbackStrategy::GlobalReset,
        RollbackStrategy::Manual => proto::RollbackStrategy::Manual,
        RollbackStrategy::Abort => proto::RollbackStrategy::Abort,
    }
}

fn status_to_proto(status: StepStatus) -> proto::StepStatus {
    match status {
        StepStatus::Success => proto::StepStatus::Success,
        StepStatus::RollbackTriggered => proto::StepStatus::RollbackTriggered,
        StepStatus::Failed => proto::StepStatus::Failed,
    }
}

fn step_response(outcome: StepOutcome) -> Result<ExecuteStepResponse, Status> {
    let output_json = match &outcome.output {
        Some(value) => serde_json::to_string(value)
            .map_err(|e| Status::internal(format!("failed to encode output: {e}")))?,
        None => String::new(),
    };
    let (clean_hint, error_category) = match &outcome.hint {
        Some(hint) => (hint.text.clone(), hint.category.as_str().to_owned()),
        None => (String::new(), String::new()),
    };
    Ok(ExecuteStepResponse {
        status: status_to_proto(outcome.status) as i32,
        output_json,
        clean_hint,
        strategy: strategy_to_proto(outcome.strategy) as i32,
        rollback_to_step: outcome.rollback_to_step,
        next_step_id: outcome.next_step,
        constraints: outcome.constraints,
        context_reset: outcome.context_reset,
        error_category,
        rollback_latency_ms: outcome.rollback_latency.as_secs_f64() * 1_000.0,
        abort_reason: outcome.abort_reason.unwrap_or_default(),
        compensation_errors: outcome.compensation_failures,
    })
}

#[tonic::async_trait]
impl AgentTxService for AgentTxGrpcService {
    async fn begin_transaction(
        &self,
        request: Request<BeginTransactionRequest>,
    ) -> Result<Response<BeginTransactionResponse>, Status> {
        let req = request.into_inner();
        if req.metadata.len() > MAX_METADATA_ENTRIES {
            return Err(Status::invalid_argument(format!(
                "metadata exceeds {MAX_METADATA_ENTRIES} entries"
            )));
        }
        let defaults = self.engine.config().default_policy;
        let policy = req.policy.map(|p| policy_from_proto(p, defaults));
        let outcome = self.engine.begin(&req.agent_id, req.metadata, policy).await?;
        Ok(Response::new(BeginTransactionResponse {
            transaction_id: outcome.tx_id,
            next_step_id: outcome.next_step,
        }))
    }

    async fn execute_step(
        &self,
        request: Request<ExecuteStepRequest>,
    ) -> Result<Response<ExecuteStepResponse>, Status> {
        let req = request.into_inner();
        require_tx_id(&req.transaction_id)?;
        if req.step_id == 0 {
            return Err(Status::invalid_argument("step_id must be >= 1"));
        }
        if req.tool_name.is_empty() || req.tool_name.len() > MAX_TOOL_NAME_BYTES {
            return Err(Status::invalid_argument(format!(
                "tool_name must be 1-{MAX_TOOL_NAME_BYTES} bytes"
            )));
        }
        if req.arguments_json.len() > MAX_ARGUMENTS_BYTES {
            return Err(Status::invalid_argument(format!(
                "arguments_json exceeds {MAX_ARGUMENTS_BYTES} bytes"
            )));
        }
        if req.raw_context.len() > MAX_CONTEXT_BYTES {
            return Err(Status::invalid_argument(format!(
                "raw_context exceeds {MAX_CONTEXT_BYTES} bytes"
            )));
        }
        let outcome = self
            .engine
            .execute_step(StepRequest {
                tx_id: req.transaction_id,
                step_id: req.step_id,
                tool_name: req.tool_name,
                arguments_json: req.arguments_json,
                raw_context: req.raw_context,
            })
            .await?;
        Ok(Response::new(step_response(outcome)?))
    }

    async fn commit_transaction(
        &self,
        request: Request<CommitTransactionRequest>,
    ) -> Result<Response<CommitTransactionResponse>, Status> {
        let req = request.into_inner();
        require_tx_id(&req.transaction_id)?;
        let outcome = self.engine.commit(&req.transaction_id).await?;
        Ok(Response::new(CommitTransactionResponse {
            committed: true,
            steps_committed: outcome.steps_committed,
            effects_dispatched: outcome.effects_dispatched,
            dispatch_errors: outcome.dispatch_errors,
        }))
    }

    async fn rollback_transaction(
        &self,
        request: Request<RollbackTransactionRequest>,
    ) -> Result<Response<RollbackTransactionResponse>, Status> {
        let req = request.into_inner();
        require_tx_id(&req.transaction_id)?;
        let outcome = self
            .engine
            .rollback(&req.transaction_id, req.to_step, &req.reason)
            .await?;
        Ok(Response::new(RollbackTransactionResponse {
            rolled_back: true,
            next_step_id: outcome.next_step,
            compensations_run: outcome.compensations_run,
            compensation_errors: outcome.compensation_failures,
        }))
    }

    async fn get_transaction(
        &self,
        request: Request<GetTransactionRequest>,
    ) -> Result<Response<GetTransactionResponse>, Status> {
        let req = request.into_inner();
        require_tx_id(&req.transaction_id)?;
        let view = self.engine.get(&req.transaction_id).await?;
        Ok(Response::new(GetTransactionResponse {
            transaction_id: view.tx_id,
            status: view.status.as_str().to_owned(),
            next_step_id: view.next_step,
            constraints: view.constraints,
            global_resets: view.global_resets,
            context: view
                .context
                .into_iter()
                .map(|c| proto::ContextEntry {
                    step_id: c.step_id,
                    tool_name: c.tool_name,
                    raw_context: c.raw_context,
                })
                .collect(),
            agent_id: view.agent_id,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_override_keeps_unset_defaults() {
        let defaults = RollbackPolicy::default();
        let merged = policy_from_proto(
            proto::RollbackPolicy {
                max_local_depth: Some(0),
                max_global_resets: None,
                replay_budget_factor: None,
            },
            defaults,
        );
        assert_eq!(merged.max_local_depth, 0);
        assert_eq!(merged.max_global_resets, defaults.max_global_resets);
        assert_eq!(merged.replay_budget_factor, defaults.replay_budget_factor);
    }

    #[test]
    fn enums_map_to_proto() {
        assert_eq!(strategy_to_proto(RollbackStrategy::DependencyJump), proto::RollbackStrategy::DependencyJump);
        assert_eq!(status_to_proto(StepStatus::RollbackTriggered), proto::StepStatus::RollbackTriggered);
    }
}
