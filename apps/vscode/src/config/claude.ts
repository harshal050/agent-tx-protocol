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

/**
 * Claude Code user settings (`~/.claude.json`, key `mcpServers`), shared by the
 * Claude Code extension and CLI. This is the entry `claude mcp add --scope user`
 * writes. Other keys, their order and the file's indentation are preserved.
 */
export const claudeFormat: ConfigFormat = { read, upsert, remove };

function read(text: string | undefined): EntryState {
  if (text === undefined || text.trim() === "") return { kind: "missing" };
  let doc: Record<string, unknown>;
  try {
    doc = parseObject(text);
  } catch (error) {
    return { kind: "invalid", message: firstLine(error) };
  }
  const servers = doc.mcpServers;
  const server = isRecord(servers) ? servers[SERVER_NAME] : undefined;
  if (server === undefined) return { kind: "missing" };
  const entry = toEntry(server);
  if (!entry) return { kind: "invalid", message: `mcpServers.${SERVER_NAME} has no command.` };
  return { kind: "present", entry, enabled: true };
}

function upsert(text: string | undefined, entry: ServerEntry): string {
  const doc = text === undefined || text.trim() === "" ? {} : parseObject(text);
  const servers = isRecord(doc.mcpServers) ? doc.mcpServers : {};
  servers[SERVER_NAME] = { type: "stdio", command: entry.command, args: entry.args, env: {} };
  doc.mcpServers = servers;
  return serialize(doc, text);
}

function remove(text: string): string {
  const doc = parseObject(text);
  if (!isRecord(doc.mcpServers) || !(SERVER_NAME in doc.mcpServers)) return text;
  delete doc.mcpServers[SERVER_NAME];
  return serialize(doc, text);
}

function parseObject(text: string): Record<string, unknown> {
  let doc: unknown;
  try {
    doc = JSON.parse(text);
  } catch (error) {
    throw new ConfigError(
      `.claude.json has a mistake, so AgentTx can't change it safely: ${firstLine(error)}`,
    );
  }
  if (!isRecord(doc)) throw new ConfigError(".claude.json should contain a JSON object.");
  return doc;
}

function serialize(doc: Record<string, unknown>, original: string | undefined): string {
  const indent = /^\{\r?\n([ \t]+)"/.exec(original ?? "")?.[1] ?? 2;
  const json = JSON.stringify(doc, null, indent);
  return original === undefined || original.endsWith("\n") ? `${json}\n` : json;
}
