import path from "node:path";

import { claudeFormat } from "./config/claude";
import { codexFormat } from "./config/codex";
import { type ConfigFormat, type ServerEntry } from "./config/common";
import { readText, writeTextAtomic } from "./config/files";
import { type ClientId, type Host, clientDataHome, displayPath, firstLine } from "./host";
import { resolveCommand } from "./program";

/** An AI app whose MCP servers live in a settings file in the user's home folder. */
export interface FileClient {
  id: Exclude<ClientId, "github-copilot">;
  label: string;
  format: ConfigFormat;
  configPath(host: Host): string;
}

export const codexClient: FileClient = {
  id: "codex",
  label: "Codex",
  format: codexFormat,
  // Same lookup as the Codex extension: CODEX_HOME, else ~/.codex.
  configPath: (host) =>
    path.join(host.env.CODEX_HOME?.trim() || path.join(host.home, ".codex"), "config.toml"),
};

export const claudeCodeClient: FileClient = {
  id: "claude-code",
  label: "Claude Code",
  format: claudeFormat,
  // Same lookup as Claude Code: CLAUDE_CONFIG_DIR, else the home folder.
  configPath: (host) => path.join(host.env.CLAUDE_CONFIG_DIR?.trim() || host.home, ".claude.json"),
};

export type StatusKind = "connected" | "not-connected" | "disabled" | "needs-repair" | "error";

export interface ClientStatus {
  kind: StatusKind;
  detail: string;
  configPath?: string;
}

export function serverEntry(client: ClientId, host: Host, exe: string): ServerEntry {
  return { command: exe, args: ["mcp", "--home", clientDataHome(host, client)] };
}

export async function fileClientStatus(client: FileClient, host: Host): Promise<ClientStatus> {
  const configPath = client.configPath(host);
  let text: string | undefined;
  try {
    text = await readText(configPath);
  } catch (error) {
    return { kind: "error", detail: `Can't read ${configPath}: ${firstLine(error)}`, configPath };
  }

  const state = client.format.read(text);
  if (state.kind === "invalid") {
    return {
      kind: "error",
      detail: `${path.basename(configPath)} has a mistake: ${state.message}`,
      configPath,
    };
  }
  if (state.kind === "missing")
    return { kind: "not-connected", detail: "Not connected yet.", configPath };

  const program = await resolveCommand(state.entry.command, host);
  if (!program) {
    return {
      kind: "needs-repair",
      detail: `${client.label} is set to start "${state.entry.command}", but that program can't be found, so ${client.label} can't start AgentTx.`,
      configPath,
    };
  }
  if (!state.enabled) {
    return {
      kind: "disabled",
      detail: `AgentTx is added but turned off in ${client.label}.`,
      configPath,
    };
  }
  return { kind: "connected", detail: `Starts ${displayPath(program, host)}`, configPath };
}

export async function connectFileClient(
  client: FileClient,
  host: Host,
  exe: string,
): Promise<void> {
  const configPath = client.configPath(host);
  const text = await readText(configPath);
  await writeTextAtomic(configPath, client.format.upsert(text, serverEntry(client.id, host, exe)));
}

/** Returns false when AgentTx wasn't set up in that app. */
export async function disconnectFileClient(client: FileClient, host: Host): Promise<boolean> {
  const configPath = client.configPath(host);
  const text = await readText(configPath);
  if (text === undefined) return false;
  const next = client.format.remove(text);
  if (next === text) return false;
  await writeTextAtomic(configPath, next);
  return true;
}
