//! # AgentTx-Protocol
//!
//! ACID transactional proxy for LLM agent tool execution.
//!
//! * [`storage`] — RocksDB overlay state with undo-journal snapshots.
//! * [`ledger`] — step dependency DAG and Saga compensation / staging.
//! * [`parser`] — deterministic error → Clean Hint extraction.
//! * [`engine`] — transaction lifecycle and hybrid cascading rollbacks.
//! * [`grpc`] — tonic transport.
//!
//! ## Embedding
//!
//! ```no_run
//! use std::sync::Arc;
//! use agenttx::config::EngineConfig;
//! use agenttx::engine::{Engine, ToolRegistry, builtin_tools};
//! use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
//! use agenttx::storage::{RocksStore, StoreOptions};
//!
//! # async fn run() -> anyhow::Result<()> {
//! let store = Arc::new(RocksStore::open("data/rocksdb", StoreOptions::default())?);
//! let tools = ToolRegistry::new();
//! builtin_tools::register_builtin_tools(&tools, "data/sandbox");
//! let undo = UndoRegistry::new();
//! builtin_tools::register_builtin_undo_factories(&undo);
//! let engine = Arc::new(Engine::new(
//!     store,
//!     tools,
//!     undo,
//!     Arc::new(OutboxDispatcher::new("data/outbox.jsonl")),
//!     EngineConfig::default(),
//! ));
//! engine.recover().await?;
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:50051").await?;
//! agenttx::grpc::serve(engine, listener, std::future::pending()).await?;
//! # Ok(()) }
//! ```

pub mod config;
pub mod connect;
pub mod engine;
pub mod errors;
pub mod grpc;
pub mod ledger;
pub mod mcp;
pub mod parser;
pub mod storage;

pub use config::{Config, EngineConfig};
pub use engine::Engine;
pub use errors::{AgentTxError, Result};
pub use grpc::proto;
