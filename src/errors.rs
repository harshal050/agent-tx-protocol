//! Crate-wide error type and its mapping onto gRPC status codes.

use thiserror::Error;

/// Convenience alias used throughout the crate.
pub type Result<T, E = AgentTxError> = std::result::Result<T, E>;

/// Infrastructure and protocol errors.
///
/// Tool failures caused by the agent (bad arguments, constraint violations,
/// ...) are *not* represented here: they flow through the rollback engine and
/// are reported inside a successful RPC response. `AgentTxError` covers
/// everything that prevents the proxy itself from doing its job.
#[derive(Debug, Error)]
pub enum AgentTxError {
    #[error("storage error: {0}")]
    Storage(#[from] rocksdb::Error),

    #[error("column family `{0}` is missing")]
    MissingColumnFamily(&'static str),

    #[error("corrupt record at `{key}`: {reason}")]
    Corrupt { key: String, reason: String },

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("transaction `{0}` not found")]
    TransactionNotFound(String),

    #[error("transaction `{id}` is {status}, expected {expected}")]
    InvalidTransactionState {
        id: String,
        status: String,
        expected: &'static str,
    },

    #[error("illegal transaction state transition {from} -> {to}")]
    IllegalTransition { from: String, to: String },

    #[error("step {got} is out of order: expected step {expected}")]
    StepOutOfOrder { expected: u32, got: u32 },

    #[error("snapshot for step {step_id} of transaction `{tx_id}` not found")]
    SnapshotNotFound { tx_id: String, step_id: u32 },

    #[error("write conflict on key `{key}`: it was committed by another transaction")]
    WriteConflict { key: String },

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("compensation failed for `{action}`: {reason}")]
    Compensation { action: String, reason: String },

    #[error("configuration error: {0}")]
    Config(String),

    #[error("background task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

impl From<AgentTxError> for tonic::Status {
    fn from(err: AgentTxError) -> Self {
        use AgentTxError::*;
        let message = err.to_string();
        match err {
            TransactionNotFound(_) => tonic::Status::not_found(message),
            InvalidTransactionState { .. } | IllegalTransition { .. } | StepOutOfOrder { .. } => {
                tonic::Status::failed_precondition(message)
            }
            InvalidArgument(_) => tonic::Status::invalid_argument(message),
            WriteConflict { .. } => tonic::Status::aborted(message),
            Storage(_)
            | MissingColumnFamily(_)
            | Corrupt { .. }
            | Serialization(_)
            | Io(_)
            | SnapshotNotFound { .. }
            | Compensation { .. }
            | Config(_)
            | Join(_) => tonic::Status::internal(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_protocol_errors_to_grpc_codes() {
        let cases = [
            (AgentTxError::TransactionNotFound("t".into()), tonic::Code::NotFound),
            (
                AgentTxError::StepOutOfOrder { expected: 2, got: 5 },
                tonic::Code::FailedPrecondition,
            ),
            (AgentTxError::InvalidArgument("x".into()), tonic::Code::InvalidArgument),
            (AgentTxError::WriteConflict { key: "k".into() }, tonic::Code::Aborted),
            (AgentTxError::Config("bad".into()), tonic::Code::Internal),
        ];
        for (err, code) in cases {
            assert_eq!(tonic::Status::from(err).code(), code);
        }
    }
}
