/** The command an AI app runs to start AgentTx. */
export interface ServerEntry {
  command: string;
  args: string[];
}

export type EntryState =
  | { kind: "missing" }
  | { kind: "present"; entry: ServerEntry; enabled: boolean }
  | { kind: "invalid"; message: string };

/** Reads and edits the AgentTx entry in one app's settings file without touching anything else. */
export interface ConfigFormat {
  read(text: string | undefined): EntryState;
  upsert(text: string | undefined, entry: ServerEntry): string;
  remove(text: string): string;
}

/** A settings file this extension can't change safely. The message is shown to the user. */
export class ConfigError extends Error {
  override name = "ConfigError";
}

export const SERVER_NAME = "agenttx";

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function toEntry(value: unknown): ServerEntry | undefined {
  if (!isRecord(value) || typeof value.command !== "string") return undefined;
  const args = Array.isArray(value.args)
    ? value.args.filter((arg): arg is string => typeof arg === "string")
    : [];
  return { command: value.command, args };
}
