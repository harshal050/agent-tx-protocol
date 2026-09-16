import * as vscode from "vscode";

import { SetupController, clientInfo } from "./controller";
import { type ClientId, displayPath, firstLine, isClientId } from "./host";
import { SetupViewProvider } from "./view";

export const GUIDE_URL = "https://agenttx.site/docs/connect-vscode-extension";
const WELCOME_SHOWN = "agenttx.welcomeShown";

export function activate(context: vscode.ExtensionContext): void {
  const controller = new SetupController();
  const view = new SetupViewProvider(context.extensionUri, controller);
  context.subscriptions.push(
    controller,
    view,
    vscode.window.registerWebviewViewProvider(SetupViewProvider.viewId, view),
  );

  const command = (name: string, handler: (...args: unknown[]) => unknown) =>
    context.subscriptions.push(vscode.commands.registerCommand(name, handler));

  command("agenttx.openSetup", () =>
    vscode.commands.executeCommand("workbench.view.extension.agenttx"),
  );
  command("agenttx.refresh", () => controller.refresh());
  command("agenttx.openGuide", () => vscode.env.openExternal(vscode.Uri.parse(GUIDE_URL)));

  command("agenttx.install", async () => {
    try {
      const target = await vscode.window.withProgress(
        { location: vscode.ProgressLocation.Notification, title: "Installing AgentTx" },
        (progress) => controller.install((message) => progress.report({ message })),
      );
      notify(
        `AgentTx is installed (${displayPath(target, controller.host)}). Next, connect your AI agent.`,
      );
    } catch (error) {
      showError(error);
    }
  });

  command("agenttx.test", async () => {
    const result = await controller.runTest();
    if (result.kind === "ok") notify(result.detail);
    else showError(new Error(result.detail));
  });

  command("agenttx.connect", async (id?: unknown) => {
    const client = isClientId(id) ? id : await pickClient(controller, "Connect AgentTx to…");
    if (!client) return;
    try {
      const hint = await controller.connect(client);
      const action =
        client === "codex"
          ? "Reload Window"
          : client === "github-copilot"
            ? "Open Chat"
            : undefined;
      notify(`AgentTx is connected to ${clientInfo(client).label}. ${hint}`, action, () =>
        vscode.commands.executeCommand(
          client === "codex" ? "workbench.action.reloadWindow" : "workbench.action.chat.open",
        ),
      );
    } catch (error) {
      showError(error);
    }
  });

  command("agenttx.disconnect", async (id?: unknown) => {
    const client = isClientId(id) ? id : await pickClient(controller, "Disconnect AgentTx from…");
    if (!client) return;
    try {
      await controller.disconnect(client);
      notify(`AgentTx is disconnected from ${clientInfo(client).label}.`);
    } catch (error) {
      showError(error);
    }
  });

  command("agenttx.openConfig", async (id?: unknown) => {
    const configPath = controller.snapshot().clients.find((client) => client.id === id)?.configPath;
    if (!configPath) return;
    try {
      await vscode.window.showTextDocument(vscode.Uri.file(configPath));
    } catch (error) {
      showError(error);
    }
  });

  command("agenttx.openExtension", (id?: unknown) => {
    const extensionId = isClientId(id) ? clientInfo(id).extensionId : undefined;
    if (extensionId) return vscode.commands.executeCommand("extension.open", extensionId);
  });

  command("agenttx.copyRequest", async () => {
    await vscode.env.clipboard.writeText(controller.snapshot().request);
    vscode.window.setStatusBarMessage(
      "$(check) AgentTx request copied. Paste it into your AI chat.",
      4000,
    );
  });

  registerStatusBar(context, controller);
  registerCopilotServer(context, controller);

  context.subscriptions.push(
    vscode.window.onDidChangeWindowState((state) => {
      if (state.focused) void controller.refresh();
    }),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("agenttx")) void controller.refresh();
    }),
    vscode.extensions.onDidChange(() => void controller.refresh()),
  );

  void controller.refresh().then(() => welcome(context, controller));
}

export function deactivate(): void {}

function registerStatusBar(context: vscode.ExtensionContext, controller: SetupController): void {
  const item = vscode.window.createStatusBarItem(
    "agenttx.status",
    vscode.StatusBarAlignment.Right,
    100,
  );
  item.name = "AgentTx";
  item.command = "agenttx.openSetup";
  const update = () => {
    const { program, clients } = controller.snapshot();
    const connected = clients
      .filter((client) => client.status === "connected")
      .map((client) => client.label);
    const broken = clients.some(
      (client) => client.status === "needs-repair" || client.status === "error",
    );
    if (program.kind === "checking") return;
    item.text = broken
      ? "$(warning) AgentTx"
      : connected.length > 0
        ? "$(pass-filled) AgentTx"
        : "$(plug) AgentTx";
    item.tooltip = broken
      ? "AgentTx needs attention. Click to fix."
      : connected.length > 0
        ? `AgentTx is connected to ${connected.join(", ")}`
        : "Connect AgentTx to your AI agent";
    item.show();
  };
  context.subscriptions.push(item, controller.onDidChange(update));
}

/** Makes AgentTx available to GitHub Copilot Chat through VS Code's own MCP support. */
function registerCopilotServer(
  context: vscode.ExtensionContext,
  controller: SetupController,
): void {
  const lm = vscode.lm as Partial<typeof vscode.lm> | undefined;
  if (typeof lm?.registerMcpServerDefinitionProvider !== "function") return;

  const changed = new vscode.EventEmitter<void>();
  let last = JSON.stringify(controller.copilotServer() ?? null);
  context.subscriptions.push(
    changed,
    controller.onDidChange(() => {
      const next = JSON.stringify(controller.copilotServer() ?? null);
      if (next !== last) {
        last = next;
        changed.fire();
      }
    }),
    lm.registerMcpServerDefinitionProvider("agenttx", {
      onDidChangeMcpServerDefinitions: changed.event,
      provideMcpServerDefinitions: () => {
        const server = controller.copilotServer();
        return server
          ? [
              new vscode.McpStdioServerDefinition(
                "AgentTx",
                server.command,
                server.args,
                {},
                server.version,
              ),
            ]
          : [];
      },
    }),
  );
}

async function pickClient(
  controller: SetupController,
  title: string,
): Promise<ClientId | undefined> {
  const labels: Record<string, string> = {
    connected: "Connected",
    "not-connected": "Not connected",
    disabled: "Turned off",
    "needs-repair": "Needs repair",
    error: "Problem",
  };
  const picked = await vscode.window.showQuickPick(
    controller.snapshot().clients.map((client) => ({
      label: client.label,
      description: labels[client.status],
      detail: client.description,
      id: client.id,
    })),
    { title, placeHolder: "Choose your AI agent" },
  );
  return picked?.id;
}

async function welcome(
  context: vscode.ExtensionContext,
  controller: SetupController,
): Promise<void> {
  if (context.globalState.get<boolean>(WELCOME_SHOWN)) return;
  await context.globalState.update(WELCOME_SHOWN, true);
  if (controller.snapshot().clients.some((client) => client.status === "connected")) return;
  notify(
    "AgentTx is ready. Connect Codex, Claude Code or GitHub Copilot in one click.",
    "Open Setup",
    () => vscode.commands.executeCommand("agenttx.openSetup"),
  );
}

function notify(message: string, action?: string, onAction?: () => unknown): void {
  const shown = action
    ? vscode.window.showInformationMessage(message, action)
    : vscode.window.showInformationMessage(message);
  void shown.then((choice) => {
    if (choice === action) void onAction?.();
  });
}

function showError(error: unknown): void {
  void vscode.window
    .showErrorMessage(`AgentTx: ${firstLine(error)}`, "Open the Guide")
    .then((choice) => {
      if (choice) void vscode.commands.executeCommand("agenttx.openGuide");
    });
}
