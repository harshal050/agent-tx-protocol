---
title: Introduction
description: AgentTx wraps every LLM tool call in an ACID transaction, finds the step that really caused a failure, rewinds state in milliseconds and hands the agent a one-line fix.
---

AgentTx is an open-source transactional proxy for LLM agents, written in Rust. Your agent sends tool calls to AgentTx over gRPC instead of calling tools directly. AgentTx runs each call as a **step** inside a **transaction**, and nothing becomes permanent until you commit.

## The problem

Agents fail in ways normal retry logic can't handle:

- **Silent root causes.** Step 3 stores the wrong customer id. Nothing fails until step 9 tries to create an invoice and hits a foreign-key constraint. Retrying step 9 never helps.
- **Poisoned context.** Each failure pastes a 3 KB stack trace into the prompt. After a few retries the context is mostly noise.
- **Half-done side effects.** A restart re-sends emails, leaves draft files behind and trips over rows the first attempt already inserted.
- **Runaway loops.** "Retry, then restart from scratch" can re-execute the same steps many times over — O(n²) work for an n-step task.

## What AgentTx does

When a step fails, AgentTx runs a deterministic pipeline. No LLM is involved.

1. **Cleans the error.** A compiled rule set turns the raw trace into one actionable line:

   ```text
   Key (user_id)=(101) is not present in table "users"
   → Hint: Foreign key constraint failed for 'user_id'. Ensure target user exists before step execution.
   ```

2. **Finds where to resume.** A dependency graph of step inputs and outputs traces `user_id` back to the step that produced it.
3. **Rewinds.** It restores transaction state from an undo journal in milliseconds, runs Saga undo actions for reversible effects such as files, and discards emails and webhooks that were staged but not yet sent.
4. **Stays bounded.** A replay budget of `c·N·⌈log₂(N+1)⌉` steps, plus a cap on global resets, guarantees the loop ends.

The agent gets back `ROLLBACK_TRIGGERED`, the step to resume from, and the hint to add to its prompt.

## When to use it

AgentTx fits agents that **change things**: databases, files, tickets, payments, messages. It is most useful when:

- tasks run for many steps and depend on each other's outputs,
- a bad intermediate result is expensive to clean up by hand, or
- you need guarantees that nothing irreversible happens before a task succeeds.

A read-only research agent gains less, though it still benefits from Clean Hints.

## Next steps

- [Quickstart](./quickstart.md): run the server and trigger your first rollback in five minutes.
- [Architecture](./architecture.md): how the pieces fit together.
- [Benchmarks](https://github.com/harshal050/agent-tx-protocol/tree/main/benchmarks): measured before/after numbers, reproducible on your machine.
