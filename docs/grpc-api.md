---
title: gRPC API
description: Complete reference for agenttx.v1.AgentTxService — methods, messages, enums and status codes.
---

Package `agenttx.v1`. The source of truth is [`agenttx.proto`](../crates/agenttx-protocol/proto/agenttx/v1/agenttx.proto).

## Service

| Method | Request → Response |
|---|---|
| `BeginTransaction` | `BeginTransactionRequest` → `BeginTransactionResponse` |
| `ExecuteStep` | `ExecuteStepRequest` → `ExecuteStepResponse` |
| `CommitTransaction` | `CommitTransactionRequest` → `CommitTransactionResponse` |
| `RollbackTransaction` | `RollbackTransactionRequest` → `RollbackTransactionResponse` |
| `GetTransaction` | `GetTransactionRequest` → `GetTransactionResponse` |

The server also exposes `grpc.health.v1.Health`.

## BeginTransaction

| Field | Type | Notes |
|---|---|---|
| `agent_id` | string | up to 256 bytes, for audit |
| `metadata` | map<string, string> | up to 64 entries |
| `policy` | `RollbackPolicy` (optional) | per-transaction override |

`RollbackPolicy` fields are all optional; unset fields keep the server defaults.

| Field | Type | Range |
|---|---|---|
| `max_local_depth` | uint32 | 0–16 |
| `max_global_resets` | uint32 | 0–16 |
| `replay_budget_factor` | double | 0.1–1000 |

Response: `transaction_id` (UUID) and `next_step_id` (always `1`).

## ExecuteStep

| Field | Type | Notes |
|---|---|---|
| `transaction_id` | string | required |
| `step_id` | uint32 | must equal the last `next_step_id` |
| `tool_name` | string | 1–128 bytes |
| `arguments_json` | string | JSON object, ≤ 1 MiB; may contain `${steps.N.path}` |
| `raw_context` | string | ≤ 1 MiB; stored in the context log on success |

Response:

| Field | Type | Set when |
|---|---|---|
| `status` | `StepStatus` | always |
| `output_json` | string | `SUCCESS` |
| `clean_hint` | string | failure |
| `strategy` | `RollbackStrategy` | always (`NONE` on success) |
| `rollback_to_step` | uint32 | first undone step; `0` = initial state |
| `next_step_id` | uint32 | step to send next; `0` once the transaction ended |
| `constraints` | repeated string | aggregated hints |
| `context_reset` | bool | global reset or abort |
| `error_category` | string | failure, e.g. `foreign_key_violation` |
| `rollback_latency_ms` | double | time spent compensating and restoring |
| `abort_reason` | string | `FAILED` |
| `compensation_errors` | repeated string | undo actions that failed |

## CommitTransaction

Request: `transaction_id`. Response:

| Field | Type |
|---|---|
| `committed` | bool |
| `steps_committed` | uint32 |
| `effects_dispatched` | uint32 |
| `dispatch_errors` | repeated string |

## RollbackTransaction

| Field | Type | Notes |
|---|---|---|
| `transaction_id` | string | required |
| `to_step` | uint32 (optional) | `n > 0` rewinds to before step `n`; unset or `0` aborts |
| `reason` | string | logged |

Response: `rolled_back`, `next_step_id` (`0` if aborted), `compensations_run`, `compensation_errors`.

## GetTransaction

Returns `transaction_id`, `status` (e.g. `ACTIVE`), `next_step_id`, `constraints`, `global_resets`, `context` (a list of `{step_id, tool_name, raw_context}`) and `agent_id`. Ended transactions return `NOT_FOUND`.

## Enums

`StepStatus`: `STEP_STATUS_SUCCESS`, `STEP_STATUS_ROLLBACK_TRIGGERED`, `STEP_STATUS_FAILED`.

`RollbackStrategy`: `NONE`, `LOCAL_BACKTRACK`, `DEPENDENCY_JUMP`, `GLOBAL_RESET`, `MANUAL`, `ABORT` (each prefixed `ROLLBACK_STRATEGY_`).

## Status codes

| Code | Cause |
|---|---|
| `OK` | the call was processed, including steps that failed and rolled back |
| `INVALID_ARGUMENT` | missing ids, `step_id = 0`, oversized fields, invalid policy, `to_step` out of range |
| `NOT_FOUND` | unknown or ended transaction |
| `FAILED_PRECONDITION` | wrong step order, transaction not active |
| `ABORTED` | write-write conflict at commit (the transaction was compensated) |
| `INTERNAL` | storage, I/O or serialization failure |
