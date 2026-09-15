---
title: Configuration
description: Every server flag and environment variable, with defaults.
---

Every setting can be passed as a command-line flag or an environment variable. Flags take precedence.

```bash
agenttx --listen-addr 0.0.0.0:50051 --sync-writes --log-format json
# or
AGENTTX_LISTEN_ADDR=0.0.0.0:50051 AGENTTX_SYNC_WRITES=true agenttx
```

## Server

| Flag | Environment variable | Default | Description |
|---|---|---|---|
| `--listen-addr` | `AGENTTX_LISTEN_ADDR` | `127.0.0.1:50051` | gRPC bind address |
| `--log-format` | `AGENTTX_LOG_FORMAT` | `text` | `text` or `json`; level via `RUST_LOG` |

## Storage

| Flag | Environment variable | Default | Description |
|---|---|---|---|
| `--data-dir` | `AGENTTX_DATA_DIR` | `data/rocksdb` | RocksDB directory |
| `--sync-writes` | `AGENTTX_SYNC_WRITES` | `false` | fsync the WAL on every write |
| `--fs-root` | `AGENTTX_FS_ROOT` | `data/sandbox` | sandbox for `fs.*` tools |
| `--outbox-path` | `AGENTTX_OUTBOX_PATH` | `data/outbox.jsonl` | committed non-reversible effects |

## Rollback policy

| Flag | Environment variable | Default | Description |
|---|---|---|---|
| `--max-local-depth` | `AGENTTX_MAX_LOCAL_DEPTH` | `2` | local backtrack depth D (0–16) |
| `--max-global-resets` | `AGENTTX_MAX_GLOBAL_RESETS` | `1` | resets before abort (0–16) |
| `--replay-budget-factor` | `AGENTTX_REPLAY_BUDGET_FACTOR` | `2.0` | `c` in `c·N·⌈log₂(N+1)⌉` |

Clients can override these per transaction. See [Rollback strategies](./rollback-strategies.md).

## Execution

| Flag | Environment variable | Default | Description |
|---|---|---|---|
| `--step-timeout-ms` | `AGENTTX_STEP_TIMEOUT_MS` | `30000` | per-tool deadline |
| `--tx-idle-timeout-secs` | `AGENTTX_TX_IDLE_TIMEOUT_SECS` | `900` | abort idle transactions; `0` disables |
| `--compensation-retries` | `AGENTTX_COMPENSATION_RETRIES` | `3` | attempts per undo action |
| `--max-constraints` | `AGENTTX_MAX_CONSTRAINTS` | `32` | aggregated hints kept (oldest evicted first) |

## Validation

The server refuses to start on invalid combinations: a zero timeout, zero compensation retries, zero max constraints, or policy values out of range.
