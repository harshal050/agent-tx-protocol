---
title: Built-in tools
description: Arguments, outputs, errors and rollback behavior of the tools that ship with AgentTx.
---

The built-in tools are production-usable primitives and double as reference implementations for your own tools. Their error messages deliberately mirror real systems (PostgreSQL, POSIX, serde), so the error cleaner produces precise hints.

## kv.put

Stores a JSON value in transactional state.

| Argument | Type | Required |
|---|---|---|
| `key` | string | yes |
| `value` | any JSON | no (defaults to `null`) |

Output: `{ "key", "value" }`. **Rollback:** journal snapshot.

## kv.get

| Argument | Type | Required |
|---|---|---|
| `key` | string | yes |

Output: `{ "key", "value" }`. Fails with `no row with key=… in table "kv"` when the key is missing.

## kv.delete

Output: `{ "key", "deleted": bool }`. **Rollback:** journal snapshot (a tombstone is written).

## record.insert

Inserts a row, enforcing its primary key and any declared foreign keys.

| Argument | Type | Required | Notes |
|---|---|---|---|
| `table` | string | yes | `[A-Za-z0-9_]{1,128}` |
| `id` | string or number | yes | primary key |
| `fields` | object | no | row columns |
| `references` | object | no | `{ "field": "referenced_table" }` |

Output: the row plus `table` and `id`.

| Error | Message (abridged) |
|---|---|
| duplicate id | `duplicate key value violates unique constraint "t_pkey" … Key (id)=(…) already exists.` |
| missing referenced field | `null value in column "field" of relation "t" violates not-null constraint` |
| referenced row missing | `… violates foreign key constraint "t_field_fkey" … Key (field)=(…) is not present in table "ref".` |

**Rollback:** journal snapshot.

## record.get

| Argument | Type | Required |
|---|---|---|
| `table` | string | yes |
| `id` | string or number | yes |

Output: the stored row. Fails with `no row with id=… in table "…"`.

## fs.write

Writes a UTF-8 file inside the sandbox (`--fs-root`), atomically via a temp file and rename.

| Argument | Type | Required |
|---|---|---|
| `path` | string | yes; relative, no `..` |
| `contents` | string | yes |

Output: `{ "path", "bytes" }`. **Rollback:** Saga undo restores the previous bytes, or deletes the file if it didn't exist. The action is crash-recoverable.

## fs.read

Output: `{ "path", "contents" }`. Fails with `No such file or directory: '…'`. Read-only.

## effect.stage

Stages a non-reversible effect for dispatch after commit.

| Argument | Type | Required |
|---|---|---|
| `channel` | string | yes (e.g. `email`, `webhook`) |
| `payload` | any JSON | no |
| `idempotency_key` | string | no (generated if omitted) |

Output: `{ "staged": true, "channel", "idempotency_key" }`. **Rollback:** discarded; nothing was sent.
