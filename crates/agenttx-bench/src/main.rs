//! `agenttx-bench`: reproducible benchmarks for AgentTx.
//!
//! Every number is measured on the machine running the harness, except the
//! explicitly labeled *modeled* end-to-end latency, which adds an assumed LLM
//! latency per tool call (`--llm-step-ms`) to the measured wall time.
//!
//! ```text
//! cargo run --release -p agenttx-bench -- --out benchmarks/results/latest.json
//! ```

mod cleaner;
mod env;
mod fixtures;
mod latency;
mod report;
mod restore;
mod sim;
mod stats;
mod throughput;

use std::path::PathBuf;

use clap::{Parser, ValueEnum};

use report::{Report, RunConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Suite {
    Simulation,
    Latency,
    Restore,
    Cleaner,
    Throughput,
}

#[derive(Debug, Parser)]
#[command(
    name = "agenttx-bench",
    version,
    about = "Benchmark agent tool loops with and without AgentTx"
)]
struct Args {
    /// Output JSON path.
    #[arg(long, default_value = "benchmarks/results/latest.json")]
    out: PathBuf,

    /// Fewer iterations and smaller sizes (CI smoke runs).
    #[arg(long)]
    quick: bool,

    /// Assumed LLM latency per tool call for the modeled end-to-end latency.
    #[arg(long, default_value_t = 1000)]
    llm_step_ms: u64,

    /// Suites to run (comma-separated). Defaults to all.
    #[arg(long, value_enum, value_delimiter = ',')]
    suites: Vec<Suite>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = RunConfig {
        quick: args.quick,
        llm_step_ms: args.llm_step_ms,
    };
    let selected = |suite: Suite| args.suites.is_empty() || args.suites.contains(&suite);
    let mut report = Report::new(env::detect(), config);

    if selected(Suite::Simulation) {
        eprintln!("▶ simulation: agent recovery, before vs after");
        report.simulation = sim::run(&config).await?;
    }
    if selected(Suite::Latency) {
        eprintln!("▶ latency: per-step overhead");
        report.step_latency = latency::run(&config).await?;
    }
    if selected(Suite::Restore) {
        eprintln!("▶ restore: snapshot restore and commit scaling");
        report.snapshot_restore = restore::run_restore(&config)?;
        report.commit = restore::run_commit(&config)?;
    }
    if selected(Suite::Cleaner) {
        eprintln!("▶ cleaner: raw errors → clean hints");
        report.error_cleaner = cleaner::run(&config);
    }
    if selected(Suite::Throughput) {
        eprintln!("▶ throughput: concurrent transactions");
        report.throughput = throughput::run(&config).await?;
    }

    if let Some(parent) = args.out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut json = serde_json::to_string_pretty(&report)?;
    json.push('\n');
    std::fs::write(&args.out, json)?;
    print_summary(&report);
    eprintln!("✔ wrote {}", args.out.display());
    Ok(())
}

fn print_summary(report: &Report) {
    for s in &report.simulation {
        eprintln!(
            "  {:<24} baseline {:>4} calls {:>6} B feedback {:>3} dup | agenttx {:>4} calls {:>6} B feedback {:>3} dup",
            s.id,
            s.baseline.tool_executions,
            s.baseline.context_feedback_bytes,
            s.baseline.duplicate_side_effects,
            s.agenttx.tool_executions,
            s.agenttx.context_feedback_bytes,
            s.agenttx.duplicate_side_effects,
        );
    }
    for l in &report.step_latency {
        eprintln!(
            "  {:<8} p50 {:>9.1} µs  p99 {:>9.1} µs",
            l.mode, l.summary.p50_us, l.summary.p99_us
        );
    }
    for r in &report.snapshot_restore {
        eprintln!(
            "  restore {:>7} entries  p50 {:>9.2} ms",
            r.journal_entries,
            r.restore.p50_us / 1000.0
        );
    }
}
