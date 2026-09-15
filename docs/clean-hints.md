---
title: Clean Hints
description: The deterministic error cleaner that reduces stack traces to one actionable line, with no LLM involved.
---

Raw errors are a poor prompt. A JDBC stack trace can be 3 KB of frames, and only one line matters. AgentTx's error cleaner turns any failure into a **Clean Hint**:

```text
Hint: Foreign key constraint failed for 'user_id'. Ensure target user exists before step execution.
```

A hint always starts with `Hint: `, never contains a newline and is at most 240 characters. It carries:

| Field | Example | Used for |
|---|---|---|
| `text` | the line above | the agent's prompt |
| `category` | `foreign_key_violation` | `error_category` in the response, metrics |
| `rule` | `pg_foreign_key` | debugging which rule matched |
| `key` | `user_id` | root-cause search in the dependency graph |

## How matching works

1. The input is bounded to 64 KiB. Oversized inputs keep their head and tail, where exceptions usually are.
2. About 30 rules are compiled into one `RegexSet` and evaluated in a single pass. When several rules match, the rule listed first wins, so specific rules come before generic ones.
3. The winning rule's regex extracts captures, and its renderer builds the hint.
4. With no match, a fallback picks the most useful line: the last `Caused by:` (Java), otherwise the last exception line (Python), otherwise the first line that isn't a stack frame.

It takes microseconds per error and has no network dependency.

## Rule coverage

| Family | Examples |
|---|---|
| SQL constraints | PostgreSQL, MySQL, SQLite foreign key, unique, not-null |
| Schema | missing column, missing table |
| Payload | serde `missing field`, pydantic `Field required`, JSON Schema required |
| Runtime | Python `KeyError`, missing positional argument, `'NoneType' has no attribute`, JavaScript property access on undefined, Java NPE |
| Values | invalid integer literal, serde type mismatch |
| Filesystem | not found, permission denied, sandbox escape |
| Network | HTTP 401/403, 404, other 4xx, 5xx, 429 rate limits, timeouts, connection refused |
| Protocol | unknown tool, unresolved `${steps…}` reference, malformed JSON |

The full table lives in [`crates/agenttx-protocol/src/parser/rules.rs`](../crates/agenttx-protocol/src/parser/rules.rs).

## Writing hints that work

Hints are written for an LLM that has to change its next tool call:

- **Name the thing.** Say `'customer_id'`, not "a column".
- **Say what to do.** "Ensure target customer exists before step execution."
- **Never include values that could be secrets.** Rules only echo keys and short, non-sensitive values.

## Adding a rule

Add a `Rule` to `RULES` in `parser/rules.rs`. Put specific patterns above generic ones, name the capture group that holds the offending key in `key_groups`, and add a case to the table test in `parser/error_cleaner.rs`.
