import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { errorCode } from "./config/files";
import { type Host, binaryName, installedBinaryPath } from "./host";

export const RELEASES_URL =
  "https://github.com/harshal050/agent-tx-protocol/releases/latest/download";

/** A problem the user can act on. The message is shown as is. */
export class InstallError extends Error {
  override name = "InstallError";
}

export interface ReleaseAsset {
  target: string;
  archive: "tar.gz" | "zip";
}

/** Matches the builds published by .github/workflows/release.yml. */
export function releaseAsset(platform: NodeJS.Platform, arch: string): ReleaseAsset | undefined {
  if (platform === "linux" && arch === "x64")
    return { target: "x86_64-unknown-linux-gnu", archive: "tar.gz" };
  if (platform === "linux" && arch === "arm64")
    return { target: "aarch64-unknown-linux-gnu", archive: "tar.gz" };
  if (platform === "darwin" && arch === "arm64")
    return { target: "aarch64-apple-darwin", archive: "tar.gz" };
  if (platform === "win32" && arch === "x64")
    return { target: "x86_64-pc-windows-msvc", archive: "zip" };
  return undefined;
}

export interface InstallOptions {
  fetch?: typeof fetch;
  baseUrl?: string;
  onProgress?: (message: string) => void;
}

/**
 * Downloads the latest release for this computer, checks its SHA-256 checksum
 * and installs the program into `~/.agenttx/bin`, like scripts/install.sh.
 */
export async function installProgram(host: Host, options: InstallOptions = {}): Promise<string> {
  const asset = releaseAsset(host.platform, host.arch);
  if (!asset) {
    throw new InstallError(
      `There is no ready-made AgentTx download for ${host.platform} (${host.arch}) yet. The guide shows how to build it from source.`,
    );
  }
  const fetchImpl = options.fetch ?? globalThis.fetch;
  const baseUrl = options.baseUrl ?? RELEASES_URL;
  const name = `agenttx-${asset.target}`;
  const file = `${name}.${asset.archive}`;

  options.onProgress?.(`Downloading ${file}…`);
  const [archive, checksum] = await Promise.all([
    download(fetchImpl, `${baseUrl}/${file}`),
    download(fetchImpl, `${baseUrl}/${file}.sha256`),
  ]);
  const expected = checksum.toString("utf8").trim().split(/\s+/)[0]?.toLowerCase();
  const actual = createHash("sha256").update(archive).digest("hex");
  if (!expected || expected !== actual) {
    throw new InstallError(
      "The download was damaged (its checksum doesn't match). Please try again.",
    );
  }

  options.onProgress?.("Unpacking…");
  const work = await fs.mkdtemp(path.join(os.tmpdir(), "agenttx-install-"));
  try {
    const archivePath = path.join(work, file);
    await fs.writeFile(archivePath, archive);
    await run("tar", [asset.archive === "zip" ? "-xf" : "-xzf", archivePath, "-C", work]);

    const target = installedBinaryPath(host);
    const temp = `${target}.download`;
    await fs.mkdir(path.dirname(target), { recursive: true });
    await fs.copyFile(path.join(work, name, binaryName(host)), temp);
    await fs.chmod(temp, 0o755);
    try {
      await fs.rename(temp, target);
    } catch (error) {
      await fs.rm(temp, { force: true });
      if (errorCode(error) === "EBUSY" || errorCode(error) === "EPERM") {
        throw new InstallError(
          "AgentTx is running in another app right now. Close that app (or its chat), then try again.",
        );
      }
      throw error;
    }
    return target;
  } finally {
    await fs.rm(work, { recursive: true, force: true }).catch(() => undefined);
  }
}

async function download(fetchImpl: typeof fetch, url: string): Promise<Buffer> {
  let response: Response;
  try {
    response = await fetchImpl(url, { redirect: "follow" });
  } catch (error) {
    throw new InstallError(
      `Couldn't download AgentTx. Check your internet connection. (${error instanceof Error ? error.message : String(error)})`,
    );
  }
  if (response.status === 404) {
    throw new InstallError(
      "No AgentTx release has been published for download yet. Build it from source instead (see the guide).",
    );
  }
  if (!response.ok) throw new InstallError(`Couldn't download AgentTx (HTTP ${response.status}).`);
  return Buffer.from(await response.arrayBuffer());
}

function run(command: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    execFile(command, args, { windowsHide: true }, (error, _stdout, stderr) => {
      if (error)
        reject(new InstallError(`Couldn't unpack the download: ${stderr.trim() || error.message}`));
      else resolve();
    });
  });
}
