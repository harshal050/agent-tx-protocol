import { parse } from "smol-toml";

import { firstLine } from "../host";
import {
  ConfigError,
  type ConfigFormat,
  type EntryState,
  SERVER_NAME,
  type ServerEntry,
  isRecord,
  toEntry,
} from "./common";

const MARKER = "# Added by the AgentTx VS Code extension";
/** `[mcp_servers.agenttx]`, `[mcp_servers."agenttx"]` and sub-tables such as `[mcp_servers.agenttx.env]`. */
const OWN_TABLE =
  /^\s*\[\s*mcp_servers\s*\.\s*(?:agenttx|"agenttx"|'agenttx')\s*(?:\.[^\]]*)?\]\s*(?:#.*)?$/;
const ANY_TABLE = /^\s*\[/;

/**
 * Codex settings (`~/.codex/config.toml`), shared by the Codex extension, the
 * Codex CLI and the ChatGPT desktop app. Edits are line based so comments and
 * the user's own formatting survive; every result is parsed again before it
 * is written.
 */
export const codexFormat: ConfigFormat = { read, upsert, remove };

function read(text: string | undefined): EntryState {
  if (text === undefined) return { kind: "missing" };
  let doc: Record<string, unknown>;
  try {
    doc = parse(text);
  } catch (error) {
    return { kind: "invalid", message: firstLine(error) };
  }
  const servers = doc.mcp_servers;
  const server = isRecord(servers) ? servers[SERVER_NAME] : undefined;
  if (server === undefined) return { kind: "missing" };
  const entry = toEntry(server);
  if (!entry) return { kind: "invalid", message: `[mcp_servers.${SERVER_NAME}] has no command.` };
  return { kind: "present", entry, enabled: !(isRecord(server) && server.enabled === false) };
}

function upsert(text: string | undefined, entry: ServerEntry): string {
  const base = text === undefined ? "" : removeTables(text);
  const before = read(base);
  if (before.kind === "invalid") {
    throw new ConfigError(
      `config.toml has a mistake, so AgentTx can't add itself safely: ${before.message}`,
    );
  }
  if (before.kind === "present") {
    throw new ConfigError(
      `config.toml sets up "${SERVER_NAME}" in a way this extension can't edit. Remove that entry by hand, then try again.`,
    );
  }

  const block = [
    MARKER,
    `[mcp_servers.${SERVER_NAME}]`,
    `command = ${tomlString(entry.command)}`,
    `args = [${entry.args.map(tomlString).join(", ")}]`,
  ].join("\n");
  const next = base.trim() === "" ? `${block}\n` : `${base.trimEnd()}\n\n${block}\n`;

  const after = read(next);
  if (after.kind !== "present" || after.entry.command !== entry.command) {
    throw new ConfigError(
      "AgentTx could not write a valid config.toml entry. Nothing was changed.",
    );
  }
  return next;
}

function remove(text: string): string {
  if (!text.split(/\r?\n/).some((line) => OWN_TABLE.test(line))) return text;
  const next = removeTables(text).trimEnd();
  return next === "" ? "" : `${next}\n`;
}

function removeTables(text: string): string {
  const kept: string[] = [];
  let inside = false;
  for (const line of text.split(/\r?\n/)) {
    if (ANY_TABLE.test(line)) inside = OWN_TABLE.test(line);
    if (inside || line.trim() === MARKER) continue;
    kept.push(line);
  }
  return kept.join("\n").replace(/\n{3,}/g, "\n\n");
}

/** A JSON string is also a valid TOML basic string (quotes, backslashes and control characters are escaped). */
function tomlString(value: string): string {
  return JSON.stringify(value);
}
