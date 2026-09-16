import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  claudeCodeClient,
  codexClient,
  connectFileClient,
  disconnectFileClient,
  fileClientStatus,
} from "../clients";
import { BACKUP_SUFFIX } from "../config/files";
import type { Host } from "../host";

let root: string;
let host: Host;
let exe: string;

beforeEach(async () => {
  root = await fs.mkdtemp(path.join(os.tmpdir(), "agenttx-vscode-"));
  host = {
    env: { PATH: path.join(root, "empty") },
    home: path.join(root, "home"),
    platform: process.platform,
    arch: process.arch,
  };
  exe = path.join(host.home, ".agenttx", "bin", "agenttx");
  await fs.mkdir(path.dirname(exe), { recursive: true });
  await fs.writeFile(exe, "#!/bin/sh\n", { mode: 0o755 });
});

afterEach(() => fs.rm(root, { recursive: true, force: true }));

describe.each([codexClient, claudeCodeClient])("$label", (client) => {
  it("connects, reports the status and disconnects", async () => {
    expect((await fileClientStatus(client, host)).kind).toBe("not-connected");

    await connectFileClient(client, host, exe);
    expect(await fileClientStatus(client, host)).toMatchObject({
      kind: "connected",
      detail: `Starts ${path.join("~", ".agenttx", "bin", "agenttx")}`,
      configPath: client.configPath(host),
    });
    const written = await fs.readFile(client.configPath(host), "utf8");
    expect(written).toContain(path.join(host.home, ".agenttx", "clients", client.id));

    // Every later change keeps a copy of the previous file.
    await connectFileClient(client, host, exe);
    await expect(fs.readFile(`${client.configPath(host)}${BACKUP_SUFFIX}`, "utf8")).resolves.toBe(
      written,
    );

    expect(await disconnectFileClient(client, host)).toBe(true);
    expect((await fileClientStatus(client, host)).kind).toBe("not-connected");
    expect(await disconnectFileClient(client, host)).toBe(false);
  });

  it("flags a setup that starts a program the app can't find", async () => {
    const file = client.configPath(host);
    await fs.mkdir(path.dirname(file), { recursive: true });
    await fs.writeFile(
      file,
      client.format.upsert(undefined, { command: "agenttx", args: ["mcp"] }),
    );
    expect(await fileClientStatus(client, host)).toMatchObject({ kind: "needs-repair" });

    // Repair: connecting again writes the full path.
    await connectFileClient(client, host, exe);
    expect(await fileClientStatus(client, host)).toMatchObject({ kind: "connected" });
  });

  it("finds a bare command on PATH", async () => {
    const file = client.configPath(host);
    await fs.mkdir(path.dirname(file), { recursive: true });
    await fs.writeFile(
      file,
      client.format.upsert(undefined, { command: "agenttx", args: ["mcp"] }),
    );
    host.env.PATH = path.dirname(exe);
    expect(await fileClientStatus(client, host)).toMatchObject({ kind: "connected" });
  });
});
