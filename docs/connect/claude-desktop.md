---
title: Connect Claude Desktop
description: Add AgentTx to the Claude Desktop app for macOS and Windows by editing one settings file.
---

[Claude Desktop](https://claude.ai/download) is the Claude app for macOS and Windows. Desktop apps don't read your terminal's settings, so you give Claude the **full path** to AgentTx.

## Before you start

- [ ] AgentTx is installed and `agenttx --version` works ([overview](./overview.md#step-1-install-agenttx)).
- [ ] Claude Desktop is installed and up to date (Claude menu → **Check for Updates…**).

## Step 1 — Copy your settings

In a terminal, run:

```bash
agenttx connect claude-desktop
```

It prints a block of settings that already contains the full path to AgentTx on your computer. It looks like this, with your own path:

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

Copy the block from **your** terminal, not from this page.

## Step 2 — Open Claude Desktop's settings file

1. Click the **Claude** menu in your computer's menu bar (not the settings inside the chat window) and choose **Settings…**
2. In the left sidebar, choose **Developer**.
3. Click **Edit Config**.

This opens `claude_desktop_config.json` in a text editor. The file is here:

| Computer | Location |
|---|---|
| macOS | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Windows | `%APPDATA%\Claude\claude_desktop_config.json` |

## Step 3 — Paste and save

- **If the file is empty** (or only contains `{}`), replace everything with the block you copied.
- **If it already has an `"mcpServers"` section**, add only the `"agenttx": { … }` part inside it, with a comma between servers.

Save the file.

> [!WARNING]
> The file must stay valid JSON: every `{` needs a matching `}`, and servers are separated by commas. If Claude Desktop shows a settings error after restarting, paste the file into a JSON checker such as [jsonlint.com](https://jsonlint.com).

## Step 4 — Restart Claude Desktop

Quit Claude Desktop completely (on macOS: **Claude → Quit Claude**; on Windows: right-click the tray icon → **Quit**), then open it again.

## Step 5 — Check the connection

1. In a new chat, click the **+** button in the bottom-left corner of the message box.
2. Hover over **Connectors** and click **Manage connectors**.
3. Select **agenttx**. You should see its tools: `begin_transaction`, `run_step`, `commit_transaction` and the others.

## Step 6 — Try it

Paste this into a new chat:

```text
Use the AgentTx tools for this task. Begin a transaction, save the customer id
"c-404" with kv.put, then create invoice "inv-1" for that customer with
record.insert (reference customer_id to the customers table).
If AgentTx rolls back, follow its hint, then commit the transaction.
```

Claude asks for permission before each tool call. Click **Allow**. Then follow [Your first safe task](./first-task.md) to understand the results.

## Remove AgentTx

Delete the `"agenttx": { … }` entry from `claude_desktop_config.json`, save and restart Claude Desktop.

## Having trouble?

Claude Desktop writes connection logs here:

- macOS: `~/Library/Logs/Claude/mcp-server-agenttx.log`
- Windows: `%APPDATA%\Claude\logs\mcp-server-agenttx.log`

See [Troubleshooting](./troubleshooting.md) for what the messages mean.
