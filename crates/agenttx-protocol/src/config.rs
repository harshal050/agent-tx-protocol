//! Runtime configuration (CLI flags and `AGENTTX_*` environment variables).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, ValueEnum};

use crate::engine::state_machine::RollbackPolicy;
use crate::errors::{AgentTxError, Result};
use crate::storage::StoreOptions;

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    Text,
    Json,
}

/// Top-level server configuration.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "agenttx",
    version,
    about = "ACID transactional gRPC proxy for LLM agent tool execution"
)]
pub struct Config {
    /// Address the gRPC server binds to.
    #[arg(long, env = "AGENTTX_LISTEN_ADDR", default_value = "127.0.0.1:50051")]
    pub listen_addr: SocketAddr,

    /// RocksDB data directory.
    #[arg(long, env = "AGENTTX_DATA_DIR", default_value = "data/rocksdb")]
    pub data_dir: PathBuf,

    /// Sandbox root for the built-in `fs.*` tools.
    #[arg(long, env = "AGENTTX_FS_ROOT", default_value = "data/sandbox")]
    pub fs_root: PathBuf,

    /// Append-only JSONL outbox receiving committed non-reversible effects.
    #[arg(long, env = "AGENTTX_OUTBOX_PATH", default_value = "data/outbox.jsonl")]
    pub outbox_path: PathBuf,

    /// Maximum local backtrack depth D.
    #[arg(long, env = "AGENTTX_MAX_LOCAL_DEPTH", default_value_t = RollbackPolicy::DEFAULT_MAX_LOCAL_DEPTH)]
    pub max_local_depth: u32,

    /// Global Step-0 resets allowed per transaction before it is aborted.
    #[arg(long, env = "AGENTTX_MAX_GLOBAL_RESETS", default_value_t = RollbackPolicy::DEFAULT_MAX_GLOBAL_RESETS)]
    pub max_global_resets: u32,

    /// Replay budget factor `c` in `c * N * ceil(log2(N + 1))`.
    #[arg(long, env = "AGENTTX_REPLAY_BUDGET_FACTOR", default_value_t = RollbackPolicy::DEFAULT_REPLAY_BUDGET_FACTOR)]
    pub replay_budget_factor: f64,

    /// Per-step tool execution timeout in milliseconds.
    #[arg(long, env = "AGENTTX_STEP_TIMEOUT_MS", default_value_t = 30_000)]
    pub step_timeout_ms: u64,

    /// Idle transactions are aborted after this many seconds (0 disables).
    #[arg(long, env = "AGENTTX_TX_IDLE_TIMEOUT_SECS", default_value_t = 900)]
    pub tx_idle_timeout_secs: u64,

    /// Attempts per Saga undo action before it is reported as failed.
    #[arg(long, env = "AGENTTX_COMPENSATION_RETRIES", default_value_t = 3)]
    pub compensation_retries: u32,

    /// Maximum aggregated hint constraints kept per transaction.
    #[arg(long, env = "AGENTTX_MAX_CONSTRAINTS", default_value_t = 32)]
    pub max_constraints: usize,

    /// fsync the RocksDB WAL on every write (power-loss durability).
    #[arg(long, env = "AGENTTX_SYNC_WRITES", default_value_t = false)]
    pub sync_writes: bool,

    /// Log output format.
    #[arg(long, env = "AGENTTX_LOG_FORMAT", value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,
}

impl Config {
    /// Validates cross-field invariants that clap cannot express.
    pub fn validate(&self) -> Result<()> {
        self.rollback_policy().validate()?;
        if self.step_timeout_ms == 0 {
            return Err(AgentTxError::Config("step_timeout_ms must be > 0".into()));
        }
        if self.compensation_retries == 0 {
            return Err(AgentTxError::Config(
                "compensation_retries must be > 0".into(),
            ));
        }
        if self.max_constraints == 0 {
            return Err(AgentTxError::Config("max_constraints must be > 0".into()));
        }
        Ok(())
    }

    pub fn rollback_policy(&self) -> RollbackPolicy {
        RollbackPolicy {
            max_local_depth: self.max_local_depth,
            max_global_resets: self.max_global_resets,
            replay_budget_factor: self.replay_budget_factor,
        }
    }

    pub fn engine_config(&self) -> EngineConfig {
        EngineConfig {
            default_policy: self.rollback_policy(),
            step_timeout: Duration::from_millis(self.step_timeout_ms),
            tx_idle_timeout: (self.tx_idle_timeout_secs > 0)
                .then(|| Duration::from_secs(self.tx_idle_timeout_secs)),
            compensation_retries: self.compensation_retries,
            max_constraints: self.max_constraints,
        }
    }

    pub fn store_options(&self) -> StoreOptions {
        StoreOptions {
            sync_writes: self.sync_writes,
            ..StoreOptions::default()
        }
    }
}

/// Options for `agenttx mcp` (the MCP server started by AI apps).
#[derive(Debug, Clone, clap::Args)]
pub struct McpArgs {
    /// Folder for AgentTx data. Defaults to ~/.agenttx
    #[arg(long, env = "AGENTTX_HOME")]
    pub home: Option<PathBuf>,

    /// Folder the fs.* tools may read and write. Defaults to <home>/workspace
    #[arg(long, env = "AGENTTX_WORKSPACE")]
    pub workspace: Option<PathBuf>,

    /// Per-step tool timeout in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    pub step_timeout_ms: u64,
}

/// Resolved folders for `agenttx mcp`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpPaths {
    pub home: PathBuf,
    pub data_dir: PathBuf,
    pub workspace: PathBuf,
    pub outbox: PathBuf,
}

impl McpArgs {
    /// Resolves folders. AI apps often start servers from an unknown working
    /// directory, so defaults live under the user's home folder, not `.`.
    pub fn resolve(&self) -> Result<McpPaths> {
        let home = match &self.home {
            Some(home) => home.clone(),
            None => std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|base| PathBuf::from(base).join(".agenttx"))
                .ok_or_else(|| {
                    AgentTxError::Config(
                        "could not find your home folder; pass --home <folder>".into(),
                    )
                })?,
        };
        Ok(McpPaths {
            data_dir: home.join("rocksdb"),
            workspace: self
                .workspace
                .clone()
                .unwrap_or_else(|| home.join("workspace")),
            outbox: home.join("outbox.jsonl"),
            home,
        })
    }
}

/// Settings consumed by [`crate::engine::Engine`].
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub default_policy: RollbackPolicy,
    pub step_timeout: Duration,
    pub tx_idle_timeout: Option<Duration>,
    pub compensation_retries: u32,
    pub max_constraints: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            default_policy: RollbackPolicy::default(),
            step_timeout: Duration::from_secs(30),
            tx_idle_timeout: Some(Duration::from_secs(900)),
            compensation_retries: 3,
            max_constraints: 32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_validate() {
        let cfg = Config::try_parse_from(["agenttx"]).expect("defaults parse");
        cfg.validate().expect("defaults are valid");
        assert_eq!(cfg.rollback_policy(), RollbackPolicy::default());
    }

    #[test]
    fn rejects_zero_timeout() {
        let cfg = Config::try_parse_from(["agenttx", "--step-timeout-ms", "0"]).expect("parses");
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn zero_idle_timeout_disables_reaper() {
        let cfg =
            Config::try_parse_from(["agenttx", "--tx-idle-timeout-secs", "0"]).expect("parses");
        assert!(cfg.engine_config().tx_idle_timeout.is_none());
    }
}
