import fs from "node:fs/promises";
import path from "node:path";

import { isRecord } from "./common";

/** A copy of the previous settings file, kept next to it before every change. */
export const BACKUP_SUFFIX = ".agenttx.bak";

export function errorCode(error: unknown): string | undefined {
  return isRecord(error) && typeof error.code === "string" ? error.code : undefined;
}

export async function readText(file: string): Promise<string | undefined> {
  try {
    return await fs.readFile(file, "utf8");
  } catch (error) {
    if (errorCode(error) === "ENOENT") return undefined;
    throw error;
  }
}

/**
 * Backs up the current file, then replaces it in one step (write a temporary
 * file, rename it over the original) so an app never reads a half-written file.
 */
export async function writeTextAtomic(file: string, text: string): Promise<void> {
  await fs.mkdir(path.dirname(file), { recursive: true });
  let mode = 0o600;
  try {
    mode = (await fs.stat(file)).mode & 0o777;
    await fs.copyFile(file, `${file}${BACKUP_SUFFIX}`);
  } catch (error) {
    if (errorCode(error) !== "ENOENT") throw error;
  }
  const temp = `${file}.agenttx-${process.pid}.tmp`;
  await fs.writeFile(temp, text, { mode });
  try {
    await fs.rename(temp, file);
  } catch (error) {
    await fs.rm(temp, { force: true });
    throw error;
  }
}
