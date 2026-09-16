import os from "node:os";
import path from "node:path";

/** Everything that depends on the machine, passed explicitly so tests can use a fake home folder. */
export interface Host {
  env: NodeJS.ProcessEnv;
  home: string;
  platform: NodeJS.Platform;
  arch: string;
}

export function currentHost(): Host {
  return { env: process.env, home: os.homedir(), platform: process.platform, arch: process.arch };
}

export type ClientId = "codex" | "claude-code" | "github-copilot";

export const CLIENT_IDS: readonly ClientId[] = ["codex", "claude-code", "github-copilot"];

export function isClientId(value: unknown): value is ClientId {
  return typeof value === "string" && (CLIENT_IDS as readonly string[]).includes(value);
}

/** Same default as `agenttx mcp --home`. */
export function agenttxHome(host: Host): string {
  return host.env.AGENTTX_HOME?.trim() || path.join(host.home, ".agenttx");
}

export function binaryName(host: Host): string {
  return host.platform === "win32" ? "agenttx.exe" : "agenttx";
}

/** Where the installer (and this extension) put the program. */
export function installedBinaryPath(host: Host): string {
  return path.join(agenttxHome(host), "bin", binaryName(host));
}

/**
 * Each app gets its own AgentTx data folder, so Codex and Claude Code can run
 * at the same time without fighting over the same database lock.
 */
export function clientDataHome(host: Host, client: ClientId): string {
  return path.join(agenttxHome(host), "clients", client);
}

export function expandHome(value: string, host: Host): string {
  if (value === "~") return host.home;
  if (value.startsWith("~/") || value.startsWith("~\\"))
    return path.join(host.home, value.slice(2));
  return value;
}

/** Shows a path inside the home folder as `~/…`: shorter, and it doesn't show the user name. */
export function displayPath(file: string, host: Host): string {
  const home = host.home.replace(/[\\/]+$/, "");
  if (file === home) return "~";
  for (const separator of ["/", "\\"]) {
    if (file.startsWith(home + separator)) return `~${separator}${file.slice(home.length + 1)}`;
  }
  return file;
}

export function firstLine(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return message.split("\n")[0]?.trim() ?? "";
}
