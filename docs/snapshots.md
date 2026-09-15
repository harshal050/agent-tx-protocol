---
title: Snapshots & storage
description: The RocksDB layout, the undo journal that makes rewinds fast, isolation and conflict detection.
---

AgentTx stores all transactional state in an embedded RocksDB database with three column families.

| Column family | Keys | Contents |
|---|---|---|
| `default` | `g/{key}` | committed values, prefixed with a commit version |
| `default` | `x/{tx}/s/{key}` | a transaction's private overlay (value or tombstone + base version) |
| `default` | `x/{tx}/o/{step}` | step outputs |
| `snapshots` | `t/{tx}` | open-transaction records (for crash recovery) |
| `snapshots` | `m/{tx}/{step}` | snapshot metadata |
| `snapshots` | `j/{tx}/{seq}` | undo journal entries |
| `saga_logs` | `u/{tx}/{step}/{seq}` | persisted undo actions |

## Why an undo journal

RocksDB's `db.snapshot()` pins a *read view* at a sequence number. It is excellent for consistent reads but it cannot move the database back in time. AgentTx therefore journals every write:

- Each write stores the key's **previous value** in the journal, in the **same atomic `WriteBatch`** as the write itself.
- `create_snapshot(step)` stores only a pointer: the next journal sequence number. It costs O(1).
- `restore_snapshot(id)` iterates the journal newer than the pointer **in reverse**, puts every prior value into one `WriteBatch`, deletes those journal entries and all later snapshots, and writes the batch atomically.

The cost of a restore depends on the number of mutations since the snapshot, never on database size. The test suite enforces restoring 5,000 writes in under 50 ms; see the benchmarks for measured scaling.

`db.snapshot()` is still used where it fits: commit reads the overlay through a pinned, consistent view.

## Isolation

Writes go to the transaction's overlay and are invisible to everyone else until commit. Reads inside a transaction see its own writes first, then committed state.

This is **read committed** isolation with write-write conflict detection:

- The first write of a key records the committed version it was based on.
- At commit, each key's current committed version must still equal that base version, or the commit fails with `WriteConflict` (first committer wins).
- Commits are serialized internally, so validate-and-apply is atomic.

Reads are not validated, so this is not serializable isolation. See [Guarantees & limits](./guarantees.md).

## Durability

Every batch goes through the RocksDB write-ahead log. By default the WAL isn't fsynced on each write: the data survives a process crash but not necessarily a power loss. Pass `--sync-writes` for power-loss durability, at a latency cost.

## Crash recovery

On startup, `Engine::recover()` finds every `t/{tx}` record, rebuilds that transaction's undo actions from `saga_logs`, runs them newest-first, and discards the transaction. If a compensation fails, the record and its Saga logs are kept, so the next start retries and an operator can inspect them.
