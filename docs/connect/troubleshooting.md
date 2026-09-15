---
title: Troubleshooting connections
description: Fixes for the most common problems when connecting AgentTx to an AI app.
---

Find the message or symptom you see, then follow the fix.

## "agenttx: command not found"

Your terminal can't find the program yet.

1. **Open a new terminal window.** The installer changes settings that only apply to new windows.
2. **Check where it was installed:**
   - macOS/Linux: `ls ~/.agenttx/bin/agenttx`
   - Windows: `dir $env:USERPROFILE\.agenttx\bin\agenttx.exe`
3. If the file exists, run the `echo 'export PATH=…'` line the installer printed, then open a new terminal.
4. If you built from source, make sure `~/.cargo/bin` is on your PATH (rustup sets this up; open a new terminal).

## The app says the server failed to start, or AgentTx never appears

Desktop apps (Claude Desktop, Cursor, Windsurf, VS Code) often can't find programs by name. **Use the full path** printed by `agenttx connect <app>` instead of just `agenttx`.

Then restart the app completely.

## Gemini CLI shows agenttx as "Disabled"

Gemini turns off tools in folders you haven't trusted. Run `gemini` in your project folder and choose **Trust folder** when it asks "Do you trust the files in this folder?". Then run `gemini mcp list` again. It should say **Connected**.

## "the AgentTx data folder … is already in use"

Two apps are running AgentTx at the same time with the same data folder. Give the second app its own folder by adding `--home` after `mcp`:

```json
{
  "mcpServers": {
    "agenttx": {
      "command": "/Users/you/.agenttx/bin/agenttx",
      "args": ["mcp", "--home", "/Users/you/.agenttx-cursor"]
    }
  }
}
```

For terminal apps:

```bash
claude mcp add --scope user agenttx -- agenttx mcp --home ~/.agenttx-claude
```

## The AI ignores AgentTx and uses its own tools

Say it explicitly in your request: **"Use the AgentTx tools for this task."** Check that the AgentTx tools are enabled in your app:

| App | Where |
|---|---|
| Claude Code | `/mcp` |
| Codex | `/mcp` |
| Claude Desktop | **+** → **Connectors** → **Manage connectors** |
| Cursor | **Customize** page, toggle on |
| VS Code | **Configure Tools** in the chat box |

## "step N is out of order: expected step M"

The AI used the wrong step number. It should always use `next_step_id` from the previous AgentTx response. Tell it: *"Use the next_step_id AgentTx returned."* The AI can also call `get_transaction` to see the current step.

## "transaction … not found"

The transaction already finished (committed or rolled back), or was cancelled after an hour of inactivity. Ask the AI to call `begin_transaction` again.

## Checking it by hand

This starts AgentTx the way apps do. You should see a message saying it is waiting for an AI app:

```bash
agenttx mcp
```

Press <kbd>Ctrl</kbd> + <kbd>C</kbd> to stop it. If you see an error instead, that error explains why your app can't connect.

## Where apps keep logs

| App | Log location |
|---|---|
| Claude Desktop (macOS) | `~/Library/Logs/Claude/mcp-server-agenttx.log` |
| Claude Desktop (Windows) | `%APPDATA%\Claude\logs\mcp-server-agenttx.log` |
| Claude Code | run `claude --debug` |
| VS Code | **Output** panel → choose **MCP** |

## Still stuck?

[Open an issue](https://github.com/harshal050/agent-tx-protocol/issues/new/choose) with your app name, your operating system, and the error text or log.
