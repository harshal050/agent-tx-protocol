---
title: Connect Cursor
description: Add AgentTx to the Cursor editor for all projects or a single project.
---

[Cursor](https://cursor.com) is an AI code editor. You connect AgentTx by adding it to Cursor's `mcp.json` file.

## Before you start

- [ ] AgentTx is installed and `agenttx --version` works ([overview](./overview.md#step-1-install-agenttx)).
- [ ] Cursor is installed and you're signed in.

## Step 1 — Copy your settings

In a terminal, run:

```bash
agenttx connect cursor
```

Copy the settings block it prints. It contains the full path to AgentTx on your computer, and looks like this:

```json
{
  "mcpServers": {
    "agenttx": {
      "command": "/Users/you/.agenttx/bin/agenttx",
      "args": ["mcp"]
    }
  }
}
```

## Step 2 — Choose where to add AgentTx

| Use AgentTx in… | Edit this file |
|---|---|
| **All projects** (recommended) | `~/.cursor/mcp.json` in your home folder |
| **One project only** | `.cursor/mcp.json` inside that project folder |

In Cursor, choose **File → Open…**, go to the folder, and open (or create) `mcp.json`.

## Step 3 — Paste and save

- **Empty or new file:** paste the whole block.
- **File already has `"mcpServers"`:** add just the `"agenttx": { … }` entry inside it, separated from other servers by a comma.

Save the file.

## Step 4 — Check the connection

Open the **Customize** page from Cursor's sidebar. `agenttx` should be listed, with its toggle switched **on**. If it isn't there, restart Cursor.

## Step 5 — Try it

Open Cursor's chat in **Agent** mode and paste:

```text
Use the AgentTx tools for this task. Begin a transaction, save the customer id
"c-404" with kv.put, then create invoice "inv-1" for that customer with
record.insert (reference customer_id to the customers table).
If AgentTx rolls back, follow its hint, then commit the transaction.
```

Approve the AgentTx tool calls when Cursor asks, then read [Your first safe task](./first-task.md) to understand the results.

## Remove or pause AgentTx

- **Pause:** switch its toggle off on the **Customize** page.
- **Remove:** delete the `"agenttx"` entry from `mcp.json` and save.

## Having trouble?

See [Troubleshooting](./troubleshooting.md).
