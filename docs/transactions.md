---
title: Transactions & steps
description: The transaction lifecycle, step ordering, references between steps, and what commit and rollback do.
---

A **transaction** groups the tool calls of one agent task. A **step** is one tool call inside it. Steps are numbered from 1, and the server always tells you which step to send next.

## Lifecycle

```text
BeginTransaction ─▶ ExecuteStep(1) ─▶ ExecuteStep(2) ─▶ … ─▶ CommitTransaction
                          ▲                   │
                          └── next_step_id ◀──┘  (after a rollback)
```

| Status | Meaning |
|---|---|
| `ACTIVE` | Accepting steps, commits and rollbacks |
| `EXECUTING` | A step is running (other calls wait for it) |
| `ROLLING_BACK` | A rewind is in progress |
| `COMMITTING` | Commit is being applied |
| `COMMITTED` | Finished successfully (terminal) |
| `ROLLED_BACK` | Aborted on request or by the idle reaper (terminal) |
| `FAILED` | Aborted by the rollback controller or a commit conflict (terminal) |

Illegal transitions are rejected, so a committed transaction can never receive another step.

## Step ordering

`ExecuteStep.step_id` must equal the last `next_step_id` the server returned. Anything else is rejected with `FAILED_PRECONDITION`, which catches duplicated or out-of-order requests from buggy clients.

After a rollback, `next_step_id` moves backwards. Your agent re-plans from that step, using the returned constraints.

## References between steps

Arguments can use earlier outputs with `${steps.<id>.<path>}`:

```json
{
  "user_id": "${steps.1.id}",
  "subject": "Order ${steps.3.order.number} confirmed",
  "first_item": "${steps.3.items.0}"
}
```

- A string that is exactly one reference becomes the referenced JSON value, with its type preserved.
- A reference inside a longer string is interpolated as text.
- Numeric path segments index into arrays.
- Referencing a step that hasn't completed, or a key the output doesn't have, is a step failure. The hint names the key, and the rollback jumps to the producing step.

References are optional, but they are the most precise way to tell AgentTx how data flows. See [Dependency graph](./dependency-graph.md).

## Commit

`CommitTransaction`:

1. checks every written key for conflicts with transactions that committed in the meantime,
2. atomically publishes all writes and deletes the transaction's journal, snapshots and Saga logs, and
3. dispatches staged non-reversible effects (emails, webhooks) *after* the commit is durable.

A write-write conflict aborts the transaction, runs its compensations and returns `ABORTED`.

## Rollback on request

`RollbackTransaction` has two forms:

- `to_step: n` rewinds to just before step `n` and keeps the transaction open, with `next_step_id = n`.
- `to_step` omitted (or 0) aborts everything: compensations run, staged effects are dropped, and state is discarded.

## Context log and constraints

Each successful step stores the `raw_context` you sent with it, and `GetTransaction` returns them. Every failure adds its Clean Hint to a de-duplicated **constraints** list. Carry that list in your system prompt so the agent doesn't repeat the mistake.

## Idle transactions

Transactions with no activity for `--tx-idle-timeout-secs` (default 15 minutes) are aborted and compensated automatically.
