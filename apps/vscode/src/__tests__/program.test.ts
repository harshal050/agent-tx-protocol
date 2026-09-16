import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { afterEach, beforeEach, describe, expect, it } from "vitest";

import type { Host } from "../host";
import { InstallError, installProgram, releaseAsset } from "../install";
import { checkServer, findProgram, readVersion } from "../program";

const REAL_BINARY = fileURLToPath(new URL("../../../../target/release/agenttx", import.meta.url));
const unix = process.platform !== "win32";

let root: string;
let host: Host;

beforeEach(async () => {
  root = await fs.mkdtemp(path.join(os.tmpdir(), "agenttx-program-"));
  host = { env: { PATH: "" }, home: path.join(root, "home"), platform: "linux", arch: "x64" };
});

afterEach(() => fs.rm(root, { recursive: true, force: true }));

async function fakeProgram(file: string, version: string): Promise<void> {
  await fs.mkdir(path.dirname(file), { recursive: true });
  await fs.writeFile(file, `#!/bin/sh\necho "agenttx ${version}"\n`, { mode: 0o755 });
}

describe.skipIf(!unix)("findProgram", () => {
  it("uses the setting, then ~/.agenttx/bin, then PATH", async () => {
    const onPath = path.join(root, "bin", "agenttx");
    await fakeProgram(onPath, "1.0.0");
    host.env.PATH = path.dirname(onPath);
    expect(await findProgram(host)).toEqual({ path: onPath, source: "path" });

    const installed = path.join(host.home, ".agenttx", "bin", "agenttx");
    await fakeProgram(installed, "1.0.0");
    expect(await findProgram(host)).toEqual({ path: installed, source: "installed" });

    expect(await findProgram(host, onPath)).toEqual({ path: onPath, source: "setting" });
    await expect(readVersion(installed)).resolves.toBe("1.0.0");
  });

  it("explains a setting that points nowhere", async () => {
    const result = await findProgram(host, "~/missing/agenttx");
    expect(result.path).toBeUndefined();
    expect(result).toMatchObject({
      problem: expect.stringContaining(path.join(host.home, "missing")),
    });
  });
});

describe("releaseAsset", () => {
  it("matches the published release builds", () => {
    expect(releaseAsset("linux", "x64")?.target).toBe("x86_64-unknown-linux-gnu");
    expect(releaseAsset("linux", "arm64")?.target).toBe("aarch64-unknown-linux-gnu");
    expect(releaseAsset("darwin", "arm64")?.target).toBe("aarch64-apple-darwin");
    expect(releaseAsset("win32", "x64")).toEqual({
      target: "x86_64-pc-windows-msvc",
      archive: "zip",
    });
    expect(releaseAsset("darwin", "x64")).toBeUndefined();
  });
});

describe.skipIf(!unix)("installProgram", () => {
  const name = "agenttx-x86_64-unknown-linux-gnu";

  async function releaseArchive(): Promise<Buffer> {
    const stage = path.join(root, "stage");
    await fakeProgram(path.join(stage, name, "agenttx"), "9.9.9");
    const archive = path.join(root, `${name}.tar.gz`);
    execFileSync("tar", ["czf", archive, "-C", stage, name]);
    return fs.readFile(archive);
  }

  function server(files: Record<string, Buffer | string>): typeof fetch {
    return (async (url: string | URL | Request) => {
      const body = files[String(url).split("/").pop() ?? ""];
      return body === undefined ? new Response("", { status: 404 }) : new Response(body);
    }) as typeof fetch;
  }

  it("downloads, verifies and installs the program", async () => {
    const archive = await releaseArchive();
    const sum = createHash("sha256").update(archive).digest("hex");
    const progress: string[] = [];
    const installed = await installProgram(host, {
      fetch: server({
        [`${name}.tar.gz`]: archive,
        [`${name}.tar.gz.sha256`]: `${sum}  ${name}.tar.gz\n`,
      }),
      onProgress: (message) => progress.push(message),
    });
    expect(installed).toBe(path.join(host.home, ".agenttx", "bin", "agenttx"));
    expect(execFileSync(installed).toString()).toContain("agenttx 9.9.9");
    expect(progress).toEqual([`Downloading ${name}.tar.gz…`, "Unpacking…"]);
  });

  it("rejects a damaged download", async () => {
    const archive = await releaseArchive();
    const install = installProgram(host, {
      fetch: server({ [`${name}.tar.gz`]: archive, [`${name}.tar.gz.sha256`]: "0".repeat(64) }),
    });
    await expect(install).rejects.toThrow(/checksum/);
    expect(existsSync(path.join(host.home, ".agenttx", "bin", "agenttx"))).toBe(false);
  });

  it("explains when no release is published", async () => {
    await expect(installProgram(host, { fetch: server({}) })).rejects.toThrow(InstallError);
  });
});

describe.skipIf(!existsSync(REAL_BINARY))("checkServer", () => {
  it("talks to the real agenttx mcp server", async () => {
    const result = await checkServer(REAL_BINARY);
    expect(result.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(result.tools).toEqual(
      expect.arrayContaining(["begin_transaction", "run_step", "commit_transaction"]),
    );
  });
});
