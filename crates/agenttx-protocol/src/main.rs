//! `agenttx` binary.
//!
//! * `agenttx` — gRPC server (default).
//! * `agenttx mcp` — MCP server for AI apps (Claude Code, Codex, Cursor, …).
//! * `agenttx connect [app]` — copy-paste setup instructions.

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use agenttx::config::{Config, EngineConfig, LogFormat, McpArgs};
use agenttx::connect::{self, Client};
use agenttx::engine::{Engine, ToolRegistry, builtin_tools};
use agenttx::ledger::{OutboxDispatcher, UndoRegistry};
use agenttx::mcp::McpServer;
use agenttx::storage::{RocksStore, StoreOptions};

const GUIDE_URL: &str = "https://agenttx.site/docs/connect-ai-agents";

#[derive(Debug, Parser)]
#[command(
    name = "agenttx",
    version,
    about = "ACID transactions for AI agent tool calls",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[command(flatten)]
    server: Config,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run as an MCP server for Claude Code, Codex, Cursor, Claude Desktop and other AI apps.
    Mcp(McpArgs),
    /// Print copy-paste setup instructions for an AI app.
    Connect {
        /// App to connect. Leave out to show every app.
        #[arg(value_enum)]
        app: Option<Client>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Connect { app }) => print_connect(app),
        Some(Command::Mcp(args)) => run_mcp(args).await,
        None => run_server(cli.server).await,
    }
}

fn print_connect(app: Option<Client>) -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("finding the agenttx program path")?;
    let exe = exe.canonicalize().unwrap_or(exe);
    println!("AgentTx program: {}\n", exe.display());
    match app {
        Some(app) => print!("{}", connect::instructions(app, &exe)),
        None => print!("{}", connect::all_instructions(&exe)),
    }
    println!("\nStep-by-step guide with screenshots: {GUIDE_URL}");
    Ok(())
}

async fn run_mcp(args: McpArgs) -> anyhow::Result<()> {
    init_tracing(LogFormat::Text, "warn");
    let paths = args.resolve()?;
    for dir in [&paths.data_dir, &paths.workspace] {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    if std::io::stdin().is_terminal() {
        eprintln!(
            "AgentTx MCP server is running and waiting for an AI app.\n\
             You normally don't start this yourself — your AI app does. Run `agenttx connect` to set that up.\n\
             Data:      {}\nWorkspace: {}\nPress Ctrl-C to stop.",
            paths.data_dir.display(),
            paths.workspace.display()
        );
    }

    let store = RocksStore::open(&paths.data_dir, StoreOptions::default()).map_err(|error| {
        if error.to_string().to_ascii_lowercase().contains("lock") {
            anyhow!(
                "the AgentTx data folder {} is already in use by another running AgentTx (for example a second AI app). \
                 Give each app its own folder by adding `--home <folder>` after `mcp` in that app's settings.",
                paths.data_dir.display()
            )
        } else {
            anyhow!(error).context(format!("opening AgentTx data at {}", paths.data_dir.display()))
        }
    })?;

    let tools = ToolRegistry::new();
    builtin_tools::register_builtin_tools(&tools, &paths.workspace);
    let undo_registry = UndoRegistry::new();
    builtin_tools::register_builtin_undo_factories(&undo_registry);
    let engine = Arc::new(Engine::new(
        Arc::new(store),
        tools,
        undo_registry,
        Arc::new(OutboxDispatcher::new(&paths.outbox)),
        EngineConfig {
            step_timeout: Duration::from_millis(args.step_timeout_ms.max(1)),
            tx_idle_timeout: Some(Duration::from_secs(3600)),
            ..EngineConfig::default()
        },
    ));
    engine
        .recover()
        .await
        .context("recovering interrupted transactions")?;
    let _reaper = engine.spawn_reaper();

    agenttx::mcp::serve_stdio(McpServer::new(engine, Some(paths.workspace)))
        .await
        .context("MCP connection failed")?;
    Ok(())
}

async fn run_server(config: Config) -> anyhow::Result<()> {
    init_tracing(config.log_format, "info");
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

/// Logs always go to stderr: in MCP mode stdout carries the protocol.
fn init_tracing(format: LogFormat, default_level: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(std::io::stderr);
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
