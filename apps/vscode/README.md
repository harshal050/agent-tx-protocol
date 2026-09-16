# AgentTx for VS Code

Connect [AgentTx](https://agenttx.site) to your AI coding agent in one click. No terminal, no settings files.

AgentTx gives an AI agent safe, undoable actions. If a step fails, AgentTx undoes the damage, tells the AI in one plain sentence what went wrong, and the AI continues from the right place.

## Works with

| Agent                               | What the extension does                                           |
| ----------------------------------- | ----------------------------------------------------------------- |
| **Codex** (extension and CLI)       | Adds AgentTx to `~/.codex/config.toml`                            |
| **Claude Code** (extension and CLI) | Adds AgentTx to your Claude Code user settings (`~/.claude.json`) |
| **GitHub Copilot** (agent mode)     | Offers AgentTx through VS Code's built-in MCP support             |

## How to use it

1. Click the **AgentTx** icon in the Activity Bar (the left edge of VS Code).
2. **Install AgentTx**: downloads the program for your computer and checks the file is intact.
3. Click **Connect** next to your agent.
4. Reload the window (Codex) or start a new conversation (Claude Code).
5. Click **Copy request**, paste it into your agent's chat, and watch AgentTx work.

Step-by-step guide with pictures: <https://agenttx.site/docs/connect-vscode-extension>

## What it changes on your computer

- Installs the `agenttx` program into `~/.agenttx/bin` (only when you click **Install AgentTx**).
- Adds or removes one `agenttx` entry in the settings file of the agent you connect. Nothing else in that file changes, and the previous version is saved next to it as `*.agenttx.bak`.
- Gives each agent its own AgentTx data folder (`~/.agenttx/clients/<agent>`), so two agents can run at the same time.

It sends no telemetry. The only network request is the download from the project's GitHub Releases.

## Settings

| Setting                         | Default   | Description                                                          |
| ------------------------------- | --------- | -------------------------------------------------------------------- |
| `agenttx.path`                  | _(empty)_ | Full path to `agenttx`. Leave empty to find it automatically.        |
| `agenttx.githubCopilot.enabled` | `false`   | Offer AgentTx to GitHub Copilot Chat. Set by the **Connect** button. |

## Commands

Open the Command Palette (<kbd>Ctrl</kbd>/<kbd>⌘</kbd> + <kbd>Shift</kbd> + <kbd>P</kbd>) and type **AgentTx**: Open Setup, Install AgentTx, Connect an AI Agent, Disconnect an AI Agent, Test AgentTx, Open the Guide.

## License

Apache-2.0
