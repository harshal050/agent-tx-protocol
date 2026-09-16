import { execFile, spawn } from "node:child_process";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { type Host, binaryName, expandHome, installedBinaryPath } from "./host";

export async function isFile(file: string): Promise<boolean> {
  try {
    return (await fs.stat(file)).isFile();
  } catch {
    return false;
  }
}

/**
 * Finds the program an app starts for `command`, the way the operating system
 * does: a full path, or a bare name looked up on PATH.
 */
export async function resolveCommand(command: string, host: Host): Promise<string | undefined> {
  const cmd = expandHome(command.trim(), host);
  if (cmd === "") return undefined;
  if (path.isAbsolute(cmd)) return (await isFile(cmd)) ? cmd : undefined;
  // A relative path depends on the folder the app happens to start in.
  if (cmd.includes("/") || cmd.includes("\\")) return undefined;

  const windows = host.platform === "win32";
  const dirs = (host.env.PATH ?? host.env.Path ?? "").split(windows ? ";" : ":").filter(Boolean);
  const extensions = windows
    ? ["", ...(host.env.PATHEXT ?? ".EXE;.CMD;.BAT").split(";").map((ext) => ext.toLowerCase())]
    : [""];
  for (const dir of dirs) {
    for (const ext of extensions) {
      const candidate = path.join(dir, cmd + ext);
      if (await isFile(candidate)) return candidate;
    }
  }
  return undefined;
}

export type ProgramLookup =
  | { path: string; source: "setting" | "installed" | "path" }
  | { path?: undefined; problem?: string };

/** The `agenttx.path` setting, then `~/.agenttx/bin`, then PATH. */
export async function findProgram(host: Host, configuredPath = ""): Promise<ProgramLookup> {
  if (configuredPath.trim() !== "") {
    const configured = expandHome(configuredPath.trim(), host);
    if (await isFile(configured)) return { path: configured, source: "setting" };
    return {
      problem: `The "agenttx.path" setting points to ${configured}, but there is no program there.`,
    };
  }
  const installed = installedBinaryPath(host);
  if (await isFile(installed)) return { path: installed, source: "installed" };
  const onPath = await resolveCommand(binaryName(host).replace(/\.exe$/, ""), host);
  return onPath ? { path: onPath, source: "path" } : {};
}

export function readVersion(exe: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(exe, ["--version"], { timeout: 10_000, windowsHide: true }, (error, stdout) => {
      if (error) return reject(error);
      const version = /agenttx\s+(\S+)/.exec(stdout)?.[1];
      if (version) resolve(version);
      else reject(new Error(`unexpected answer to --version: ${stdout.trim() || "(empty)"}`));
    });
  });
}

export interface ServerCheck {
  version: string;
  tools: string[];
}

interface RpcMessage {
  id?: number;
  result?: { serverInfo?: { version?: string }; tools?: { name: string }[] };
  error?: { message?: string };
}

/**
 * Starts `agenttx mcp` exactly like an AI app does and asks for its tools.
 * Uses a throwaway data folder so it never clashes with an app already running AgentTx.
 */
export async function checkServer(exe: string, timeoutMs = 20_000): Promise<ServerCheck> {
  const home = await fs.mkdtemp(path.join(os.tmpdir(), "agenttx-check-"));
  try {
    return await handshake(exe, home, timeoutMs);
  } finally {
    await fs.rm(home, { recursive: true, force: true }).catch(() => undefined);
  }
}

function handshake(exe: string, home: string, timeoutMs: number): Promise<ServerCheck> {
  return new Promise((resolve, reject) => {
    const child = spawn(exe, ["mcp", "--home", home], { windowsHide: true });
    let buffer = "";
    let stderr = "";
    let version = "";
    let settled = false;

    const settle = (outcome: { error: Error } | { value: ServerCheck }) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      child.stdin.end();
      child.kill();
      if ("error" in outcome) reject(outcome.error);
      else resolve(outcome.value);
    };
    const send = (message: object) => child.stdin.write(`${JSON.stringify(message)}\n`);
    const timer = setTimeout(
      () =>
        settle({ error: new Error(`AgentTx did not answer within ${timeoutMs / 1000} seconds.`) }),
      timeoutMs,
    );

    child.on("error", (error) => settle({ error }));
    child.on("exit", (code) => {
      const reason = stderr.trim().split("\n").pop()?.trim();
      settle({ error: new Error(reason || `AgentTx stopped unexpectedly (exit code ${code}).`) });
    });
    child.stdin.on("error", () => undefined);
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => (stderr += chunk));
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => {
      buffer += chunk;
      let newline: number;
      while ((newline = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, newline).trim();
        buffer = buffer.slice(newline + 1);
        if (line === "") continue;
        let message: RpcMessage;
        try {
          message = JSON.parse(line) as RpcMessage;
        } catch {
          continue;
        }
        if (message.error) {
          return settle({
            error: new Error(message.error.message ?? "AgentTx returned an error."),
          });
        }
        if (message.id === 1) {
          version = message.result?.serverInfo?.version ?? "";
          send({ jsonrpc: "2.0", method: "notifications/initialized" });
          send({ jsonrpc: "2.0", id: 2, method: "tools/list" });
        } else if (message.id === 2) {
          const tools = (message.result?.tools ?? []).map((tool) => tool.name);
          settle({ value: { version, tools } });
        }
      }
    });

    send({
      jsonrpc: "2.0",
      id: 1,
      method: "initialize",
      params: {
        protocolVersion: "2025-06-18",
        capabilities: {},
        clientInfo: { name: "agenttx-vscode", version: "0.1.0" },
      },
    });
  });
}
