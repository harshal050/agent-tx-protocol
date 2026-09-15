---
title: Benchmark methodology
description: What the benchmark harness measures, how the "before" baseline is defined, and how to reproduce every number.
---

All benchmark numbers on this site come from `crates/agenttx-bench`, run on real hardware and committed as `benchmarks/results/latest.json`. The benchmarks page renders that file directly, and the environment it ran on is shown next to the results.

## Reproduce

```bash
cargo run --release -p agenttx-bench -- --out benchmarks/results/latest.json
# quicker, smaller sizes:
cargo run --release -p agenttx-bench -- --quick
# a subset:
cargo run --release -p agenttx-bench -- --suites latency,restore
```

## Suites

### 1. Agent recovery: before vs after

The same **scripted agent** and the same **tool implementations** run twice.

**Before (baseline)** is a common ReAct-style loop with no transaction layer:

- append the raw error to the context and retry the failing step, up to 3 times;
- then restart the task from step 1, up to 3 restarts;
- every tool call auto-commits; files and emails take effect immediately;
- on replay, "already exists" errors are treated as done, the usual idempotency workaround.

**After** sends the calls through the AgentTx engine with the default policy. The agent only sees Clean Hints.

The agent is deterministic, with no LLM. It fixes the root-cause step once any feedback in its context names the offending field, which is the same capability in both modes. Without a transaction layer the baseline can't *know* where to resume, so it retries and restarts.

Each workflow mixes state writes, file writes (reversible) and emails (non-reversible), and runs at 8, 16, 32 and 64 steps. Three failure patterns are measured:

| Scenario | What happens |
|---|---|
| Silent root cause | step N/4 stores a wrong customer id; step 3N/4 fails on a foreign key |
| Transient failure | step 3N/4 times out once, then succeeds |
| Unrecoverable failure | step 3N/4 always fails, so the task cannot succeed |

Metrics per run:

| Metric | Definition |
|---|---|
| Tool calls | total tool executions until success or termination |
| Feedback tokens | bytes of error feedback added to the context ÷ 4 |
| Duplicate side effects | effects dispatched more than once (same idempotency key) |
| Leaked side effects | effects dispatched by a run that did not succeed |
| Orphaned files | files left behind that the correct final state does not contain |
| Wall time | measured harness time (tools + AgentTx overhead) |
| Modeled latency | tool calls × assumed LLM latency per call (default 1 s) + wall time |

> [!NOTE]
> **Modeled latency is an estimate, not a measurement.** Real agents spend most of their time waiting for the LLM, so the harness multiplies tool calls by `--llm-step-ms`. Everything else is measured.

The transient scenario is included on purpose: a local backtrack re-runs one extra step, so AgentTx makes **one more** tool call than a plain retry there. The results show that honestly.

### 2. Per-step latency

`kv.put` executed three ways, with 5,000 samples after a 500-sample warm-up:

- **Direct:** the tool's effect as one plain, auto-committed RocksDB put — no transaction layer at all (before).
- **Engine:** in-process `Engine::execute_step` — snapshot, journal, graph, output (after, embedded).
- **gRPC:** a full `ExecuteStep` round trip over loopback HTTP/2 (after, as a proxy).

### 3. Snapshot restore and commit

- Restore time after 10 to 100,000 journaled writes (7 runs each), next to the time the original writes took.
- Commit time for 10 to 50,000 keys.

### 4. Error cleaner

Eight realistic traces (Java/JDBC, Python, Node.js, Go, Rust): raw size versus hint size, and parse latency over 20,000 iterations.

### 5. Throughput

Transactions of 20 steps plus commit, at concurrency 1, 4, 16 and 64, through the in-process engine.

## Caveats

- Results are from one machine and change with CPU, disk and load.
- The agent is scripted, so real LLMs will fix mistakes more or less reliably.
- Baselines differ in the wild. This one is deliberately simple and common, and its policy is printed next to the results.
