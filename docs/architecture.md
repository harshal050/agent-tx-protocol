---
title: Architecture
description: How a tool call flows through AgentTx — transport, engine, storage, ledger and error cleaner.
---

AgentTx is a single Rust process. You can run it as a gRPC server or embed it as a library.

```text
 Agent / orchestrator ──gRPC──▶ ┌──────────────── AgentTx ────────────────┐
                                 │  grpc::service   request validation      │
                                 │        │                                 │
                                 │  engine::Engine  per-transaction mutex   │
                                 │   ├─ executor      run tool (timeout,    │
                                 │   │                panic isolation)      │
                                 │   ├─ parser        error → Clean Hint    │
                                 │   ├─ state_machine rollback decision     │
                                 │   └─ ledger        dependency DAG, Saga, │
                                 │                    staging queue         │
                                 │        │                                 │
                                 │  storage::RocksStore overlay + journal   │
                                 └──────────────────────────────────────────┘
```

## Request lifecycle

For every `ExecuteStep`:

1. **Validate.** The step id must equal the transaction's `next_step_id`. Size limits are checked.
2. **Snapshot.** An O(1) pointer into the undo journal records the state *before* this step.
3. **Resolve references.** `${steps.N.path}` placeholders are replaced with earlier outputs, and each one is recorded as an explicit edge in the dependency graph.
4. **Execute.** The tool runs on its own Tokio task with a deadline. Panics and timeouts become ordinary step failures.
5. **Record effects.** Undo actions the tool registered are persisted to the Saga log. Non-reversible effects go to the staging queue.
6. **Succeed or recover.**
   - On success the output is stored, the graph learns its output values, and `next_step_id` advances.
   - On failure the error cleaner produces a hint, the controller picks a strategy, and the engine rewinds.

## Components

| Module | Responsibility |
|---|---|
| `grpc` | tonic service, health checking, graceful shutdown |
| `engine` | transaction table (`DashMap`), step orchestration, commit, recovery, idle reaper |
| `engine::state_machine` | transaction status transitions and the rollback controller |
| `engine::executor` | `Tool` trait, registry, timeouts, reference resolution |
| `ledger::dependency_graph` | `petgraph` DAG and root-cause search |
| `ledger::saga` | undo ledger, crash-recoverable records, staging queue, outbox |
| `parser` | `RegexSet` rules and fallback trace summarization |
| `storage` | RocksDB column families, overlay, journal, snapshots, commit |

## Concurrency model

- Every transaction has its own `tokio::sync::Mutex`, so steps within one transaction run strictly in order.
- Different transactions run fully in parallel. The transaction table is a lock-free `DashMap`.
- Heavy storage work (commit, restore, discard) runs on Tokio's blocking pool.
- Commits take a short global lock so they can validate write-write conflicts and apply atomically.

## Failure domains

| Failure | Result |
|---|---|
| Tool error, panic or timeout | Step failure: hint, rollback decision, normal RPC response |
| Invalid request (wrong step, bad policy) | gRPC error; transaction unchanged |
| Storage error during a step | Partial effects are rewound; gRPC `INTERNAL`; the step can be retried |
| Storage error during a rewind | Transaction is aborted and compensated; crash recovery retries on restart |
| Process crash | Open transactions are compensated and discarded on the next start |
