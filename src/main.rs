//! `agenttx` server binary.

use std::sync::Arc;

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use agenttx::config::{Config, LogFormat};
use agenttx::engine::{Engine, ToolRegistry, builtin_tools};
use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
use agenttx::storage::RocksStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();
    init_tracing(config.log_format);
    config.validate().context("invalid configuration")?;

    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("creating data dir {}", config.data_dir.display()))?;
    let store = Arc::new(
        RocksStore::open(&config.data_dir, config.store_options())
            .with_context(|| format!("opening RocksDB at {}", config.data_dir.display()))?,
    );

    let tools = ToolRegistry::new();
    builtin_tools::register_builtin_tools(&tools, &config.fs_root);
    let undo_registry = UndoRegistry::new();
    builtin_tools::register_builtin_undo_factories(&undo_registry);
    let dispatcher = Arc::new(OutboxDispatcher::new(&config.outbox_path));

    let engine = Arc::new(Engine::new(
        store,
        tools,
        undo_registry,
        dispatcher,
        config.engine_config(),
    ));

    let recovery = engine.recover().await.context("crash recovery failed")?;
    if recovery.transactions > 0 {
        tracing::warn!(
            transactions = recovery.transactions,
            compensations = recovery.compensations_run,
            failures = recovery.failures.len(),
            "recovered interrupted transactions"
        );
    }
    let _reaper = engine.spawn_reaper();

    let listener = TcpListener::bind(config.listen_addr)
        .await
        .with_context(|| format!("binding {}", config.listen_addr))?;
    tracing::info!(
        addr = %listener.local_addr()?,
        tools = ?engine.tools().names(),
        "AgentTx gRPC server listening"
    );

    agenttx::grpc::serve(engine, listener, shutdown_signal()).await?;
    tracing::info!("shutdown complete");
    Ok(())
}

fn init_tracing(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_target(false);
    match format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Text => builder.init(),
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(%err, "failed to listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                tracing::error!(%err, "failed to listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
    tracing::info!("shutdown signal received; draining requests");
}
