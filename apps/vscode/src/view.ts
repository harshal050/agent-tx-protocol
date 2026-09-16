import { randomBytes } from "node:crypto";

import * as vscode from "vscode";

import type { SetupController } from "./controller";

/** Messages from the panel, mapped to the extension command that handles them. */
const ACTIONS: Record<string, string> = {
  refresh: "agenttx.refresh",
  install: "agenttx.install",
  test: "agenttx.test",
  connect: "agenttx.connect",
  disconnect: "agenttx.disconnect",
  openConfig: "agenttx.openConfig",
  openExtension: "agenttx.openExtension",
  openGuide: "agenttx.openGuide",
  copyRequest: "agenttx.copyRequest",
};

/** The "Connect AI agents" panel in the AgentTx sidebar. */
export class SetupViewProvider implements vscode.WebviewViewProvider, vscode.Disposable {
  static readonly viewId = "agenttx.setup";
  private view: vscode.WebviewView | undefined;
  private readonly subscription: vscode.Disposable;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly controller: SetupController,
  ) {
    this.subscription = controller.onDidChange(() => this.post());
  }

  resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    const media = vscode.Uri.joinPath(this.extensionUri, "media");
    view.webview.options = { enableScripts: true, localResourceRoots: [media] };
    view.webview.html = this.html(view.webview, media);
    view.webview.onDidReceiveMessage((message: unknown) => this.handle(message));
    view.onDidChangeVisibility(() => {
      if (view.visible) void this.controller.refresh();
    });
    view.onDidDispose(() => {
      this.view = undefined;
    });
  }

  dispose(): void {
    this.subscription.dispose();
  }

  private post(): void {
    void this.view?.webview.postMessage({ type: "state", state: this.controller.snapshot() });
  }

  private handle(message: unknown): void {
    if (typeof message !== "object" || message === null) return;
    const { type, id } = message as { type?: unknown; id?: unknown };
    if (type === "ready") {
      this.post();
      void this.controller.refresh();
      return;
    }
    const command = typeof type === "string" ? ACTIONS[type] : undefined;
    if (command)
      void vscode.commands.executeCommand(command, typeof id === "string" ? id : undefined);
  }

  private html(webview: vscode.Webview, media: vscode.Uri): string {
    const nonce = randomBytes(16).toString("base64");
    const asset = (file: string) =>
      webview.asWebviewUri(vscode.Uri.joinPath(media, file)).toString();
    return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource}; style-src ${webview.cspSource}; script-src 'nonce-${nonce}';" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <link rel="stylesheet" href="${asset("setup.css")}" />
    <title>AgentTx</title>
  </head>
  <body>
    <main id="app" data-logo="${asset("logo.svg")}" aria-live="polite"><p class="muted">Loading…</p></main>
    <script nonce="${nonce}" src="${asset("setup.js")}"></script>
  </body>
</html>`;
  }
}
