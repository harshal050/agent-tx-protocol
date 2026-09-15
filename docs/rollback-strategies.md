---
title: Rollback strategies
description: How AgentTx chooses between a dependency jump, a local backtrack, a global reset or an abort — and why the loop always terminates.
---

When step **K** fails, the rollback controller picks exactly one strategy. The decision is pure and deterministic: it depends only on the hint, the dependency graph and the transaction's retry counters.

## Decision order

```text
                ┌─────────────────────────────┐
 step K fails ─▶│ hint names a key that the   │ yes ─▶ DEPENDENCY_JUMP to root R
                │ graph traces to step R < K? │
                └──────────────┬──────────────┘
                               no
                ┌──────────────▼──────────────┐
                │ failure d of step K,        │ yes ─▶ LOCAL_BACKTRACK to K − d
                │ d ≤ max_local_depth?        │
                └──────────────┬──────────────┘
                               no
                ┌──────────────▼──────────────┐
                │ global resets left?         │ yes ─▶ GLOBAL_RESET to step 0
                └──────────────┬──────────────┘
                               no ─▶ ABORT
```

Every jump and backtrack must also fit within the **replay budget** (below). If it doesn't, the controller escalates.

### Dependency jump

If the Clean Hint names a key (for example `customer_id`) and the [dependency graph](./dependency-graph.md) traces it to an earlier producer `R`, AgentTx rewinds to just before `R`. Steps between `R` and `K` that didn't touch the bad value are simply re-executed.

Each root can be jumped to at most `max(max_local_depth, 1)` times per epoch. If the agent keeps producing the same bad value, the controller stops jumping and escalates.

### Local backtrack

With no traceable key, AgentTx assumes the problem is near the failure. On the `d`-th failure of step `K` it rewinds `d` steps, to `max(K − d, 1)`:

| Failure of K | Resume at |
|---|---|
| 1st | K − 1 |
| 2nd | K − 2 |
| 3rd (default depth 2) | escalate |

### Global reset

AgentTx compensates every Saga action, restores the Step-0 snapshot, and clears the transaction's context log. The response has `context_reset: true`: clear the agent's context window, keep the constraints, and start again from step 1.

Retry and jump counters reset, beginning a new **epoch**.

### Abort

After `max_global_resets` resets, the next escalation aborts. Everything is compensated, staged effects are dropped, state is discarded, and the step returns `FAILED` with an `abort_reason`.

## Replay budget

Rewinding from `K` to `T` costs `K − T + 1` replayed steps. Within an epoch, total replays may not exceed:

```text
budget = c · N · ⌈log₂(N + 1)⌉
```

`N` is the furthest step reached and `c` is `replay_budget_factor` (default 2.0). A rollback that would exceed the budget becomes a global reset instead.

| N | Budget (c = 2) |
|---|---|
| 5 | 30 |
| 10 | 80 |
| 50 | 600 |

### Why it terminates

In each epoch, every failure either consumes at least one step of a finite budget or escalates. Escalations are capped by `max_global_resets`. Total re-execution is therefore bounded by `(max_global_resets + 1) · (budget + N)`: O(N log N) per epoch, and O(N) per reset. A permanently failing tool can never trap the agent in an unbounded loop.

## Tuning

| Setting | Default | Raise it when | Lower it when |
|---|---|---|---|
| `max_local_depth` | 2 | failures often originate a few steps back | steps are expensive |
| `max_global_resets` | 1 | tasks are cheap and resets help | fail fast matters |
| `replay_budget_factor` | 2.0 | long tasks with several independent failures | cost must stay close to one pass |

Override per transaction with `BeginTransactionRequest.policy`; unset fields keep the server defaults.
