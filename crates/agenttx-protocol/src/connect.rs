//! `agenttx connect`: copy-paste setup instructions for AI apps.

use std::path::Path;

use clap::ValueEnum;
use serde_json::json;

/// AI apps with ready-made setup instructions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Client {
    ClaudeCode,
    Codex,
    ClaudeDesktop,
    Cursor,
    Vscode,
    Gemini,
    Windsurf,
}

impl Client {
    pub const ALL: [Client; 7] = [
        Client::ClaudeCode,
        Client::Codex,
        Client::ClaudeDesktop,
        Client::Cursor,
        Client::Vscode,
        Client::Gemini,
        Client::Windsurf,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::Codex => "OpenAI Codex CLI",
            Client::ClaudeDesktop => "Claude Desktop",
            Client::Cursor => "Cursor",
            Client::Vscode => "VS Code (GitHub Copilot)",
            Client::Gemini => "Gemini CLI",
            Client::Windsurf => "Windsurf",
        }
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Setup instructions for one app, using the absolute path of this binary.
pub fn instructions(client: Client, exe: &Path) -> String {
    let path = exe.display().to_string();
    let quoted = format!("\"{path}\"");
    let mcp_servers =
        pretty(&json!({ "mcpServers": { "agenttx": { "command": path, "args": ["mcp"] } } }));
    let body = match client {
        Client::ClaudeCode => format!(
            "Run this in a terminal:\n\n{}\n\nThen start `claude` and type /mcp — agenttx should show as connected.",
            indent(&format!(
                "claude mcp add --scope user agenttx -- {quoted} mcp"
            ))
        ),
        Client::Codex => format!(
            "Run this in a terminal:\n\n{}\n\nOr add these lines to ~/.codex/config.toml:\n\n{}\n\nThen run `codex mcp list` (or type /mcp inside Codex) — agenttx should be listed.",
            indent(&format!("codex mcp add agenttx -- {quoted} mcp")),
            indent(&format!(
                "[mcp_servers.agenttx]\ncommand = '{path}'\nargs = [\"mcp\"]"
            ))
        ),
        Client::ClaudeDesktop => format!(
            "In Claude Desktop, open the Claude menu → Settings… → Developer → Edit Config. That opens claude_desktop_config.json:\n  macOS:   ~/Library/Application Support/Claude/claude_desktop_config.json\n  Windows: %APPDATA%\\Claude\\claude_desktop_config.json\n\nPut this in the file (merge with any servers already there):\n\n{}\n\nSave, then fully quit and reopen Claude Desktop. To check it: click the + button in the message box → Connectors → Manage connectors.",
            indent(&mcp_servers)
        ),
        Client::Cursor => format!(
            "Create or open ~/.cursor/mcp.json (all projects) or .cursor/mcp.json (one project) and add:\n\n{}\n\nThen open the Customize page in Cursor's sidebar — agenttx should be listed with its toggle on.",
            indent(&mcp_servers)
        ),
        Client::Vscode => format!(
            "Run this in a terminal:\n\n{}\n\nOr create .vscode/mcp.json in your project:\n\n{}\n\nThen open Chat in Agent mode, trust the agenttx server when VS Code asks, and click Configure Tools to see its tools.",
            indent(&format!(
                "code --add-mcp '{}'",
                json!({ "name": "agenttx", "command": path, "args": ["mcp"] })
            )),
            indent(&pretty(
                &json!({ "servers": { "agenttx": { "type": "stdio", "command": path, "args": ["mcp"] } } })
            ))
        ),
        Client::Gemini => format!(
            "Run this in a terminal:\n\n{}\n\nThen run `gemini mcp list` — agenttx should be listed.",
            indent(&format!("gemini mcp add --scope user agenttx {quoted} mcp"))
        ),
        Client::Windsurf => format!(
            "Open ~/.codeium/windsurf/mcp_config.json (Settings → Cascade → MCP Servers, or the MCPs icon in the Cascade panel) and add:\n\n{}\n\nSave the file, then restart Cascade.",
            indent(&mcp_servers)
        ),
    };
    format!("── {} ──\n\n{body}\n", client.title())
}

/// Instructions for every supported app.
pub fn all_instructions(exe: &Path) -> String {
    Client::ALL
        .iter()
        .map(|client| instructions(*client, exe))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn json_snippets_are_valid_and_escape_paths() {
        let exe = PathBuf::from(r"C:\Users\Ada Lovelace\.cargo\bin\agenttx.exe");
        for client in [Client::ClaudeDesktop, Client::Cursor, Client::Windsurf] {
            let text = instructions(client, &exe);
            let start = text.find("    {").unwrap();
            let end = text.rfind('}').unwrap();
            let json: serde_json::Value = serde_json::from_str(
                &text[start..=end]
                    .lines()
                    .map(|l| l.trim_start_matches("    "))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            .unwrap();
            assert_eq!(
                json["mcpServers"]["agenttx"]["command"],
                exe.display().to_string()
            );
            assert_eq!(json["mcpServers"]["agenttx"]["args"][0], "mcp");
        }
    }

    #[test]
    fn commands_quote_the_binary_path() {
        let exe = PathBuf::from("/home/ada/.cargo/bin/agenttx");
        assert!(instructions(Client::ClaudeCode, &exe).contains(
            "claude mcp add --scope user agenttx -- \"/home/ada/.cargo/bin/agenttx\" mcp"
        ));
        assert!(
            instructions(Client::Codex, &exe)
                .contains("codex mcp add agenttx -- \"/home/ada/.cargo/bin/agenttx\" mcp")
        );
        assert!(
            instructions(Client::Gemini, &exe).contains(
                "gemini mcp add --scope user agenttx \"/home/ada/.cargo/bin/agenttx\" mcp"
            )
        );
        assert_eq!(all_instructions(&exe).matches("── ").count(), 7);
    }
}
