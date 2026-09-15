---
title: Guarantees & limits
description: Exactly what AgentTx guarantees, what it does not, and the trade-offs behind each choice.
---

## Guarantees

| Property | Guarantee |
|---|---|
| **Atomicity** | A transaction's RocksDB writes are published in one atomic batch, or not at all |
| **Consistency** | Built-in tools enforce primary and foreign keys; commits never overwrite concurrent committed writes |
| **Isolation** | Uncommitted writes are invisible to other transactions; write-write conflicts are detected (first committer wins) |
| **Durability** | Committed data is in the RocksDB WAL; with `--sync-writes` it survives power loss |
| **Termination** | Rollbacks are bounded by the replay budget and the global reset cap |
| **No early irreversible effects** | Staged effects are dispatched only after a durable commit |
| **Crash safety** | Interrupted transactions are compensated and discarded on restart |

## Limits and trade-offs

**Isolation is read committed, not serializable.** Transactions don't validate what they read, so two transactions can read the same value and write different keys based on it. Put invariants that span keys inside one tool call, or in a backend with its own constraints.

**External effects are compensated, not atomically rolled back.** An undo action can fail after all its retries (the remote API is down, the resource was changed by someone else). AgentTx reports the failure and keeps the record for follow-up, but it can't force the outside world back.

**Staged effects are dispatched at most once from the queue.** Dispatch happens after the commit is durable. A dispatch failure is reported in `dispatch_errors` and doesn't undo the commit. Use the idempotency key and an outbox relay for exactly-once delivery downstream.

**In-flight transactions don't resume after a crash.** The runtime state of a transaction (dependency graph, retry counters, context log) is in memory. After a crash, open transactions are compensated and aborted instead of continued.

**One step at a time per transaction.** Steps within a transaction are serialized. Parallel tool calls should use separate transactions, or a single tool that fans out internally.

**Root-cause search needs a key.** Dependency jumps need the error to name a key that the graph can trace. Unrecognized errors fall back to local backtracking, which is still bounded but less precise.

**No TLS or authentication built in.** Deploy behind a private network, a service mesh or an mTLS proxy.
