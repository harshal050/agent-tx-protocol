# AgentTx-Protocol

**ACID transactions for LLM agent tool execution.**

AgentTx is a Rust gRPC proxy that sits between an agent runtime and the tools the agent calls. Every tool call runs as a step inside a transaction. When a step fails, AgentTx does three things without calling an LLM:

1. It turns the raw error or stack trace into a one-line **Clean Hint**.
2. It works out *where* the agent should resume: a few steps back, the step that produced the bad data, or Step 0.
3. It rewinds the transaction's RocksDB state, undoes reversible external effects (Saga compensation) and discards effects that were staged but never sent.

The agent resumes from the step the proxy names, with the hint in its prompt. A replay budget bounds the total number of retries, so a failing agent can't get stuck in an O(n²) retry loop.

```
 Agent / orchestrator ──gRPC──▶ AgentTx ──▶ tools (state, files, APIs, email…)
                                   │
          ┌────────────────────────┼──────────────────────────┐
          ▼                        ▼                          ▼
   Error cleaner           Rollback controller          RocksDB store
   (RegexSet rules)        (DAG + replay budget)        (overlay + undo journal)
                                   │
                                   ▼
                     Saga ledger  +  staging queue (outbox)
```

## Contents

- [Quick start](#quick-start)
- [Protocol](#protocol)
- [How rollback decisions are made](#how-rollback-decisions-are-made)
- [Storage and snapshots](#storage-and-snapshots)
- [Saga compensation and staged effects](#saga-compensation-and-staged-effects)
- [Clean Hints](#clean-hints)
- [Built-in tools](#built-in-tools)
- [Embedding and custom tools](#embedding-and-custom-tools)
- [Configuration](#configuration)
- [Guarantees and limitations](#guarantees-and-limitations)
- [Development](#development)

## Quick start

**Prerequisites:** Rust 1.88+, a C++17 compiler, and `libclang` (RocksDB is compiled from source). You don't need `protoc`: protobufs are compiled in pure Rust with `protox`.

```bash
cargo run --release -- --listen-addr 127.0.0.1:50051
```

> **Error `'stdbool.h' file not found` while building `librocksdb-sys`?**
> Your libclang is installed without its resource headers. Install the matching `libclang-common-<ver>-dev` / `clang` package, or point bindgen at GCC's headers:
> ```bash
> export BINDGEN_EXTRA_CLANG_ARGS="-I$(dirname "$(gcc -print-file-name=include/stdbool.h)")"
> ```

Try it with [`grpcurl`](https://github.com/fullstorydev/grpcurl):

```bash
P="-plaintext -import-path proto -proto agenttx/v1/agenttx.proto"
TX=$(grpcurl $P -d '{"agent_id":"demo"}' 127.0.0.1:50051 agenttx.v1.AgentTxService/BeginTransaction | jq -r .transactionId)

grpcurl $P -d "{\"transaction_id\":\"$TX\",\"step_id\":1,\"tool_name\":\"kv.put\",
  \"arguments_json\":\"{\\\"key\\\":\\\"customer\\\",\\\"value\\\":\\\"c-404\\\"}\"}" \
  127.0.0.1:50051 agenttx.v1.AgentTxService/ExecuteStep

grpcurl $P -d "{\"transaction_id\":\"$TX\",\"step_id\":2,\"tool_name\":\"record.insert\",
  \"arguments_json\":\"{\\\"table\\\":\\\"invoices\\\",\\\"id\\\":\\\"i1\\\",\\\"fields\\\":{\\\"customer_id\\\":\\\"\${steps.1.value}\\\"},\\\"references\\\":{\\\"customer_id\\\":\\\"customers\\\"}}\"}" \
  127.0.0.1:50051 agenttx.v1.AgentTxService/ExecuteStep
# → status ROLLBACK_TRIGGERED, strategy DEPENDENCY_JUMP, next_step_id 1,
#   clean_hint "Hint: Foreign key constraint failed for 'customer_id'. Ensure target customer exists before step execution."
```

## Protocol

The service is defined in [`proto/agenttx/v1/agenttx.proto`](proto/agenttx/v1/agenttx.proto).

| RPC | Purpose |
|---|---|
| `BeginTransaction` | Opens a transaction, captures the Step-0 snapshot, and optionally overrides the rollback policy. |
| `ExecuteStep` | Runs one tool call. `step_id` must equal the last `next_step_id` the server returned. |
| `CommitTransaction` | Atomically publishes the transaction's state, then dispatches its staged effects. |
| `RollbackTransaction` | `to_step = n` rewinds to just before step `n` and keeps the transaction open. Omitting it aborts the transaction. |
| `GetTransaction` | Returns status, next step, constraints and the context log. |

The standard `grpc.health.v1.Health` service is also served.

### Client loop

```text
next = BeginTransaction().next_step_id            # 1
loop:
    r = ExecuteStep(next, plan_step(next, constraints))
    match r.status:
        SUCCESS            → next = r.next_step_id
        ROLLBACK_TRIGGERED → if r.context_reset: clear the agent's context window
                             add r.constraints to the system prompt
                             next = r.next_step_id     # re-plan from here
        FAILED             → transaction aborted; surface r.abort_reason
    until the plan is done
CommitTransaction()
```

An **agent-caused** failure is reported inside a successful RPC response: a bad tool name, malformed JSON, a constraint violation, a timeout or a panic. An RPC error status means a protocol or infrastructure problem:

| Code | Meaning |
|---|---|
| `NOT_FOUND` | The transaction doesn't exist or has already ended. |
| `FAILED_PRECONDITION` | The step is out of order, or the transaction isn't active. |
| `INVALID_ARGUMENT` | The request is malformed or the policy is invalid. |
| `ABORTED` | Commit hit a write-write conflict. The transaction was compensated and aborted. |
| `INTERNAL` | Storage or I/O failure. |

### Step references

String arguments can refer to earlier outputs with `${steps.<id>.<path>}`:

```json
{ "user_id": "${steps.1.id}", "note": "created by ${steps.2.profile.email}", "first": "${steps.3.items.0}" }
```

- A string that is exactly one reference is replaced by the referenced value, keeping its JSON type.
- A reference embedded in a longer string is interpolated as text.

Every reference is recorded as an explicit edge in the dependency graph. A reference to a key the producer didn't return fails with a hint and triggers a jump to the producing step.

## How rollback decisions are made

When step `K` fails, the controller in [`src/engine/state_machine.rs`](src/engine/state_machine.rs) runs this sequence:

1. **Clean the error.** The error becomes a Clean Hint, which may include the offending key (for example `user_id`). The hint is added to the transaction's de-duplicated constraint list.
2. **Dependency Jump.** If the key traces back through the DAG to an earlier producer `R < K`, rewind to just before `R`. Jumps to the same root are capped at `max(D, 1)` per epoch.
3. **Local Backtrack.** Otherwise, on the `d`-th failure of `K` with `d ≤ D`, rewind `d` steps to `max(K−d, 1)`. With the default `D = 2`, the first failure re-runs from `K−1` and the second from `K−2`.
4. **Global Step-0 Reset.** Otherwise, compensate every Saga action, restore Snapshot 0, clear the context log and return `context_reset = true` along with all constraints.
5. **Abort.** After `max_global_resets` resets, the next escalation compensates everything, discards the transaction and returns `FAILED`.

**Replay budget.** Rewinding from `K` to `T` costs `K − T + 1` replayed steps. If a jump or backtrack would push the epoch's total past `c · N · ⌈log₂(N+1)⌉` (`N` = furthest step reached), the controller escalates to a global reset instead. Each epoch therefore replays O(N log N) steps at most, each reset costs O(N), and the number of epochs is fixed by the policy. A permanently failing agent always terminates. `tests/rollback_simulation.rs` checks exact execution counts.

### Dependency graph

[`src/ledger/dependency_graph.rs`](src/ledger/dependency_graph.rs) uses a `petgraph::DiGraph` with one node per step. It adds edges in two ways:

- **Explicit edges** come from `${steps…}` references.
- **Implicit edges** link an input leaf to the most recent earlier output leaf with an equal scalar value, when the key names match or the input key ends in `_<output key>` (`user_id` ← `id`).

`find_root_cause(K, key)` walks backwards along edges carrying `key`, follows renames, and returns the earliest producer.

## Storage and snapshots

The store is in [`src/storage/`](src/storage/) and uses three column families:

| CF | Contents |
|---|---|
| `default` | Committed state `g/{key}` (with a commit version), per-transaction overlays `x/{tx}/s/{key}`, and step outputs |
| `snapshots` | Open-transaction records, snapshot pointers and the undo journal |
| `saga_logs` | Persisted undo actions for crash recovery |

**Why a journal instead of `db.snapshot()`:** RocksDB snapshots are read-only views pinned to a sequence number. They can't move the database back to an earlier state. AgentTx therefore records the previous value of every write in an undo journal, in the same atomic `WriteBatch` as the write:

- `create_snapshot(step)` is O(1): it just stores the current journal pointer.
- `restore_snapshot(id)` walks the journal newer than the pointer in reverse and writes all prior values in one atomic `WriteBatch`. Its cost depends on the number of writes since the snapshot, not on database size. Reverting 5,000 writes takes a few milliseconds (enforced below 50 ms in the test suite).
- `db.snapshot()` *is* used where it fits: commit reads the transaction overlay from a pinned, consistent view.

**Isolation.** A transaction's writes stay in its private overlay until commit. Reads see the transaction's own writes first, then committed state (read-committed). Commit is atomic and checks every written key against the committed version the transaction started from. If another transaction committed that key in the meantime, the later commit fails with `WriteConflict` (first committer wins).

## Saga compensation and staged effects

The ledger is in [`src/ledger/saga.rs`](src/ledger/saga.rs).

- **Reversible effects.** A tool calls `ctx.register_undo(action)` *before* performing the effect. The record is persisted to `saga_logs`. On rollback, actions for the rewound steps run newest-first (N → 1), each retried with exponential backoff. A failed compensation doesn't block the others. Its record is kept and reported back to the caller.
- **Non-reversible effects** (email, webhooks). A tool calls `ctx.stage_effect(channel, payload)`. Staged effects are dropped by any rewind past their step and dispatched only after a successful commit. The default `OutboxDispatcher` appends fsync'd JSON lines to an outbox file for a relay to deliver. Each effect carries an idempotency key.
- **Crash recovery** (presumed abort). At startup, `Engine::recover()` finds transactions still open on disk. It rebuilds their undo actions through the `UndoRegistry`, runs them, and discards the transaction state.

## Clean Hints

[`src/parser/`](src/parser/) compiles about 30 ordered rules into a single `RegexSet`. When several rules match, the first one in the list wins. The rules cover:

- PostgreSQL, MySQL and SQLite constraint errors (foreign key, unique, not-null, missing column or table)
- Python `KeyError`, `TypeError` and `AttributeError`; JavaScript null property access; Java NPEs
- serde and pydantic validation errors
- Filesystem errors, HTTP 4xx/5xx and rate limiting, timeouts, connection errors and malformed JSON

When no rule matches, the fallback picks the most useful line from the trace: the last `Caused by:` line for Java, or the last exception line for Python. Inputs are capped at 64 KiB and hints at 240 characters.

```
Key (user_id)=(101) is not present in table "users"
  → Hint: Foreign key constraint failed for 'user_id'. Ensure target user exists before step execution.
```

## Built-in tools

| Tool | Arguments | Rollback mechanism |
|---|---|---|
| `kv.put` / `kv.get` / `kv.delete` | `key`, `value` | Journal snapshot |
| `record.insert` | `table`, `id`, `fields?`, `references?: {field: table}` | Journal snapshot. Enforces primary and foreign keys and reports Postgres-style errors. |
| `record.get` | `table`, `id` | Read-only |
| `fs.write` | `path`, `contents` | Saga undo restores the previous bytes. Paths are confined to `--fs-root`. |
| `fs.read` | `path` | Read-only |
| `effect.stage` | `channel`, `payload`, `idempotency_key?` | Staged until commit |

## Embedding and custom tools

```rust
use agenttx::engine::{Tool, ToolContext, ToolError};
use async_trait::async_trait;
use serde_json::{json, Value};

struct ChargeCard { /* payment client */ }

#[async_trait]
impl Tool for ChargeCard {
    fn name(&self) -> &str { "payments.charge" }

    async fn execute(&self, ctx: &ToolContext, args: Value) -> Result<Value, ToolError> {
        let amount = args["amount"].as_u64().ok_or_else(|| ToolError::failed("missing field `amount`"))?;
        // Register the compensation first, then perform the effect.
        // ctx.register_undo(Arc::new(RefundCharge { charge_id, amount }));
        // Transactional state goes through ctx.store(); it is covered by snapshots.
        Ok(json!({ "charged": amount }))
    }
}
```

Register the tool with `ToolRegistry::register`. If its undo action should survive a crash, also register a factory for that undo kind with `UndoRegistry::register`. See the `lib.rs` docs for a complete bootstrap.

## Configuration

Every flag can also be set through an environment variable.

| Flag / env var | Default | Description |
|---|---|---|
| `--listen-addr` / `AGENTTX_LISTEN_ADDR` | `127.0.0.1:50051` | gRPC bind address |
| `--data-dir` / `AGENTTX_DATA_DIR` | `data/rocksdb` | RocksDB directory |
| `--fs-root` / `AGENTTX_FS_ROOT` | `data/sandbox` | Sandbox for the `fs.*` tools |
| `--outbox-path` / `AGENTTX_OUTBOX_PATH` | `data/outbox.jsonl` | Outbox for committed effects |
| `--max-local-depth` / `AGENTTX_MAX_LOCAL_DEPTH` | `2` | Local backtrack depth D |
| `--max-global-resets` / `AGENTTX_MAX_GLOBAL_RESETS` | `1` | Resets allowed before abort |
| `--replay-budget-factor` / `AGENTTX_REPLAY_BUDGET_FACTOR` | `2.0` | `c` in `c·N·⌈log₂(N+1)⌉` |
| `--step-timeout-ms` / `AGENTTX_STEP_TIMEOUT_MS` | `30000` | Per-tool deadline |
| `--tx-idle-timeout-secs` / `AGENTTX_TX_IDLE_TIMEOUT_SECS` | `900` | Idle transactions are aborted after this (`0` disables) |
| `--compensation-retries` / `AGENTTX_COMPENSATION_RETRIES` | `3` | Attempts per undo action |
| `--max-constraints` / `AGENTTX_MAX_CONSTRAINTS` | `32` | Constraint list cap (oldest entries are evicted first) |
| `--sync-writes` / `AGENTTX_SYNC_WRITES` | `false` | fsync the WAL on every write |
| `--log-format` / `AGENTTX_LOG_FORMAT` | `text` | `text` or `json`. Log level comes from `RUST_LOG`. |

Clients can override the rollback policy per transaction with `BeginTransactionRequest.policy`. Fields left unset keep the server defaults.

## Guarantees and limitations

- **Atomicity and durability** cover RocksDB state. Writes go through the WAL; set `--sync-writes` to survive power loss.
- **External effects are compensated, not rolled back atomically.** An undo action can fail after its retries. The failure is reported and its record is kept for an operator.
- **Staged effects are dispatched at most once** from the queue's side, *after* the commit is durable. A dispatch failure is reported in `dispatch_errors` and does not undo the commit. Use the idempotency keys downstream.
- **Isolation is read-committed with write-write conflict detection.** It is not serializable: transactions don't validate what they read.
- **Transaction runtime state lives in memory.** After a crash, open transactions are compensated and aborted rather than resumed.
- **Steps within a transaction run one at a time.** Transactions run in parallel with each other.

## Development

```bash
cargo test                 # unit tests, gRPC end-to-end tests, rollback simulations
cargo test --test rollback_simulation -- --nocapture
cargo clippy --all-targets
```

`librocksdb-sys` is built with `opt-level = 3` in dev and test profiles so that latency assertions reflect real performance. The first build takes a few minutes.

Layout:

```
proto/agenttx/v1/agenttx.proto   gRPC contract
src/grpc/                         tonic service and server bootstrap
src/engine/                       engine, state machine, executor, built-in tools
src/storage/                      RocksDB store, snapshots, undo journal
src/ledger/                       dependency DAG, Saga ledger, staging queue
src/parser/                       deterministic error cleaner
tests/                            gRPC integration and rollback simulations
```

License: Apache-2.0
