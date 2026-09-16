import type { StatusKind } from "./clients";
import type { ClientId } from "./host";

/** Everything the setup panel shows. Sent to the webview as JSON. */
export interface SetupState {
  program: ProgramState;
  clients: ClientView[];
  test?: TestState;
  request: string;
}

export interface ProgramState {
  kind: "checking" | "missing" | "found" | "broken" | "installing";
  detail: string;
  path?: string;
  /** `path` shortened with `~` for display. */
  displayPath?: string;
  version?: string;
  canInstall: boolean;
}

export interface ClientView {
  id: ClientId;
  label: string;
  description: string;
  status: StatusKind;
  detail: string;
  hint: string;
  configPath?: string;
  extensionId?: string;
  extensionInstalled: boolean;
}

export interface TestState {
  kind: "running" | "ok" | "failed";
  detail: string;
}
