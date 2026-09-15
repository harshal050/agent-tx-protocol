---
title: Saga compensation
description: Undoing reversible external effects, staging non-reversible ones until commit, and surviving crashes.
---

RocksDB state is rewound by snapshots. Effects **outside** AgentTx — files, rows in other systems, API resources, messages — need a different mechanism. AgentTx splits them into two classes.

## Reversible effects: undo actions

A tool registers a compensating action **before** it performs the effect:

```rust
ctx.register_undo(Arc::new(FileRestoreUndo { path: path.clone(), prior }));
tokio::fs::write(&path, contents).await?;
```

- The undo record is persisted to the `saga_logs` column family.
- On any rewind, the undo actions of every step at or after the target run **newest first** (N → 1).
- Each action is retried with exponential backoff up to `--compensation-retries` times. Actions must be idempotent.
- A failed compensation doesn't stop the others. It is reported in `compensation_errors`, and its record stays on disk.

Registering first means that if the tool fails halfway, the partial effect is still compensated.

## Non-reversible effects: the staging queue

Emails, webhooks and push notifications can't be un-sent, so tools never send them directly:

```rust
ctx.stage_effect("email", json!({ "to": "cfo@example.com", "template": "invoice" }), Some("invoice-42".into()));
```

- Staged effects wait in a per-transaction queue.
- A rewind past their step discards them, and so does an abort.
- They are dispatched only after `CommitTransaction` has durably committed.

The default `OutboxDispatcher` appends each committed effect as an fsynced JSON line to the outbox file. A relay process then delivers them to SMTP, webhooks or a queue — the transactional outbox pattern. Every effect carries an idempotency key for downstream de-duplication.

Implement `EffectDispatcher` to deliver effects some other way.

## Crash recovery

Closures can't be persisted, so durable undo actions implement `UndoAction` with a `kind()` and a JSON `payload()`, and you register a factory for each kind:

```rust
undo_registry.register("payments.refund", |payload| {
    Ok(Arc::new(RefundCharge::from_payload(payload)?) as Arc<dyn UndoAction>)
});
```

On startup, AgentTx rebuilds undo actions from the persisted records of every interrupted transaction and runs them. `FnUndo` (closure-based) actions are convenient for in-process effects but can't be recovered after a crash.

## Choosing the right mechanism

| Effect | Mechanism |
|---|---|
| State the agent reads back | `ctx.store()` — covered by snapshots |
| File, object storage, external row | `register_undo` with an idempotent compensation |
| Payment | `register_undo` with a refund, or stage the capture until commit |
| Email, SMS, webhook, notification | `stage_effect` |
