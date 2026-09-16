import * as vscode from "vscode";

import {
  type ClientStatus,
  type FileClient,
  claudeCodeClient,
  codexClient,
  connectFileClient,
  disconnectFileClient,
  fileClientStatus,
  serverEntry,
} from "./clients";
import { type ClientId, type Host, currentHost, displayPath, firstLine } from "./host";
import { installProgram, releaseAsset } from "./install";
import { checkServer, findProgram, readVersion } from "./program";
import type { ClientView, ProgramState, SetupState, TestState } from "./state";

export const FIRST_REQUEST =
  'Use the AgentTx tools for this task. Begin a transaction, save the customer id "c-404" with kv.put, then create invoice "inv-1" for that customer with record.insert (reference customer_id to the customers table). If AgentTx rolls back, follow its hint, then commit the transaction.';

export const COPILOT_SETTING = "githubCopilot.enabled";

interface ClientInfo {
  id: ClientId;
  label: string;
  description: string;
  /** The agent's own extension, when it isn't built into VS Code. */
  extensionId?: string;
  /** What to do after connecting, shown in the panel and the notification. */
  hint: string;
  file?: FileClient;
}

const CLIENTS: readonly ClientInfo[] = [
  {
    id: "codex",
    label: "Codex",
    description: "OpenAI's coding agent (extension and CLI)",
    extensionId: "openai.chatgpt",
    hint: "Reload the window so Codex loads AgentTx.",
    file: codexClient,
  },
  {
    id: "claude-code",
    label: "Claude Code",
    description: "Anthropic's coding agent (extension and CLI)",
    extensionId: "anthropic.claude-code",
    hint: "Start a new Claude Code conversation to use AgentTx.",
    file: claudeCodeClient,
  },
  {
    id: "github-copilot",
    label: "GitHub Copilot",
    description: "Copilot Chat in agent mode (built into VS Code)",
    hint: "In Copilot Chat, pick Agent mode and make sure AgentTx is ticked under Configure Tools.",
  },
];

export function clientInfo(id: ClientId): ClientInfo {
  const info = CLIENTS.find((client) => client.id === id);
  if (!info) throw new Error(`unknown client ${id}`);
  return info;
}

/** Holds what the panel shows and performs every action; the webview and commands only call into it. */
export class SetupController implements vscode.Disposable {
  readonly host: Host = currentHost();
  private program: ProgramState = {
    kind: "checking",
    detail: "Looking for AgentTx…",
    canInstall: false,
  };
  private readonly statuses = new Map<ClientId, ClientStatus>();
  private test: TestState | undefined;
  private refreshing: Promise<void> | undefined;
  private refreshQueued = false;
  private readonly changed = new vscode.EventEmitter<void>();
  readonly onDidChange = this.changed.event;

  get programPath(): string | undefined {
    return this.program.kind === "found" ? this.program.path : undefined;
  }

  /** Re-reads the program and every settings file. Calls made while a refresh runs trigger one more pass. */
  refresh(): Promise<void> {
    if (this.refreshing) {
      this.refreshQueued = true;
      return this.refreshing;
    }
    this.refreshing = (async () => {
      do {
        this.refreshQueued = false;
        await this.load();
      } while (this.refreshQueued);
    })().finally(() => {
      this.refreshing = undefined;
    });
    return this.refreshing;
  }

  snapshot(): SetupState {
    return {
      program: this.program,
      test: this.test,
      request: FIRST_REQUEST,
      clients: CLIENTS.map((client): ClientView => {
        const status = this.statuses.get(client.id) ?? {
          kind: "not-connected",
          detail: "Checking…",
        };
        return {
          id: client.id,
          label: client.label,
          description: client.description,
          status: status.kind,
          detail: status.detail,
          hint: client.hint,
          configPath: status.configPath,
          extensionId: client.extensionId,
          extensionInstalled:
            client.extensionId === undefined ||
            vscode.extensions.getExtension(client.extensionId) !== undefined,
        };
      }),
    };
  }

  /** Returns what the user should do next. */
  async connect(id: ClientId): Promise<string> {
    const exe = this.programPath;
    if (!exe) throw new Error("Install AgentTx first (step 1), then connect your AI agent.");
    const client = clientInfo(id);
    if (client.file) {
      await connectFileClient(client.file, this.host, exe);
    } else {
      await settings().update(COPILOT_SETTING, true, vscode.ConfigurationTarget.Global);
    }
    await this.refresh();
    return client.hint;
  }

  async disconnect(id: ClientId): Promise<void> {
    const client = clientInfo(id);
    if (client.file) await disconnectFileClient(client.file, this.host);
    else await settings().update(COPILOT_SETTING, false, vscode.ConfigurationTarget.Global);
    await this.refresh();
  }

  async install(onProgress: (message: string) => void): Promise<string> {
    this.program = { kind: "installing", detail: "Downloading AgentTx…", canInstall: true };
    this.changed.fire();
    try {
      return await installProgram(this.host, {
        onProgress: (message) => {
          this.program = { ...this.program, detail: message };
          this.changed.fire();
          onProgress(message);
        },
      });
    } finally {
      this.program = { kind: "checking", detail: "Looking for AgentTx…", canInstall: true };
      await this.refresh();
    }
  }

  async runTest(): Promise<TestState> {
    const exe = this.programPath;
    if (!exe) {
      this.test = { kind: "failed", detail: "Install AgentTx first (step 1)." };
    } else {
      this.test = { kind: "running", detail: "Starting AgentTx the way your AI app does…" };
      this.changed.fire();
      try {
        const result = await checkServer(exe);
        this.test = {
          kind: "ok",
          detail: `AgentTx ${result.version} is working. Your AI gets ${result.tools.length} tools: ${result.tools.join(", ")}.`,
        };
      } catch (error) {
        this.test = { kind: "failed", detail: `AgentTx didn't start: ${firstLine(error)}` };
      }
    }
    this.changed.fire();
    return this.test;
  }

  /** The server GitHub Copilot should start, or undefined when Copilot isn't connected. */
  copilotServer(): { command: string; args: string[]; version?: string } | undefined {
    const exe = this.programPath;
    if (!exe || !settings().get<boolean>(COPILOT_SETTING, false)) return undefined;
    return { ...serverEntry("github-copilot", this.host, exe), version: this.program.version };
  }

  dispose(): void {
    this.changed.dispose();
  }

  private async load(): Promise<void> {
    if (this.program.kind !== "installing") this.program = await this.lookupProgram();
    for (const client of CLIENTS) this.statuses.set(client.id, await this.clientStatus(client));
    this.changed.fire();
  }

  private async lookupProgram(): Promise<ProgramState> {
    const canInstall = releaseAsset(this.host.platform, this.host.arch) !== undefined;
    const found = await findProgram(this.host, settings().get<string>("path", ""));
    if (found.path === undefined) {
      return {
        kind: "missing",
        detail: found.problem ?? "AgentTx isn't installed on this computer yet.",
        canInstall,
      };
    }
    const shown = displayPath(found.path, this.host);
    try {
      const version = await readVersion(found.path);
      return {
        kind: "found",
        path: found.path,
        displayPath: shown,
        version,
        detail: `Version ${version}`,
        canInstall,
      };
    } catch (error) {
      return {
        kind: "broken",
        path: found.path,
        displayPath: shown,
        detail: `AgentTx is at ${shown} but won't start: ${firstLine(error)}`,
        canInstall,
      };
    }
  }

  private async clientStatus(client: ClientInfo): Promise<ClientStatus> {
    if (client.file) return fileClientStatus(client.file, this.host);
    if (!settings().get<boolean>(COPILOT_SETTING, false)) {
      return { kind: "not-connected", detail: "Not connected yet." };
    }
    if (!this.programPath) {
      return {
        kind: "needs-repair",
        detail: "Copilot is set to use AgentTx, but AgentTx isn't installed.",
      };
    }
    return { kind: "connected", detail: `Starts ${displayPath(this.programPath, this.host)}` };
  }
}

function settings(): vscode.WorkspaceConfiguration {
  return vscode.workspace.getConfiguration("agenttx");
}
