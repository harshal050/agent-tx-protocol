import { readFile } from "node:fs/promises";
import path from "node:path";

import { repoUrls, type ContentConfig, type ContentOrigin } from "./config";

/** Next.js extension of `fetch` options (ignored by other runtimes). */
export interface NextFetchOptions {
  revalidate?: number | false;
  tags?: string[];
}

export type FetchInit = RequestInit & { next?: NextFetchOptions };
export type FetchLike = (input: string, init?: FetchInit) => Promise<Response>;

/** Cache tag attached to every repository file request. */
export const CONTENT_CACHE_TAG = "agenttx:content";

export interface SourceFile {
  /** Repository-relative path. */
  path: string;
  content: string;
  origin: ContentOrigin;
  htmlUrl: string;
  editUrl: string;
}

export class ContentFetchError extends Error {
  override name = "ContentFetchError";
}

/** Normalizes a repository path and rejects traversal outside the repository. */
export function normalizeRepoPath(input: string): string {
  const normalized = path.posix.normalize(input.replace(/\\/g, "/")).replace(/^\/+/, "");
  if (normalized === "." || normalized === "" || normalized.split("/").includes("..")) {
    throw new Error(`invalid repository path "${input}"`);
  }
  return normalized;
}

function isMissingFile(error: unknown): boolean {
  const code = (error as NodeJS.ErrnoException | undefined)?.code;
  return code === "ENOENT" || code === "ENOTDIR" || code === "EISDIR";
}

/**
 * Reads repository files from GitHub and/or a local checkout, in the order
 * given by the configuration. A missing file in one origin falls through to
 * the next; an origin that errors is skipped with a warning unless it is the
 * only origin configured.
 */
export class ContentSource {
  readonly config: ContentConfig;
  readonly urls: ReturnType<typeof repoUrls>;
  private readonly fetchImpl: FetchLike;

  constructor(config: ContentConfig, fetchImpl?: FetchLike) {
    this.config = config;
    this.urls = repoUrls(config);
    this.fetchImpl = fetchImpl ?? ((input, init) => fetch(input, init));
  }

  async read(filePath: string): Promise<SourceFile | null> {
    const repoPath = normalizeRepoPath(filePath);
    const errors: unknown[] = [];

    for (const origin of this.config.order) {
      try {
        const content =
          origin === "github" ? await this.readGitHub(repoPath) : await this.readLocal(repoPath);
        if (content !== null) {
          return {
            path: repoPath,
            content,
            origin,
            htmlUrl: this.urls.blob(repoPath),
            editUrl: this.urls.edit(repoPath),
          };
        }
      } catch (error) {
        if (this.config.order.length === 1) throw error;
        errors.push(error);
        console.warn(
          `[content] ${origin} read failed for ${repoPath}: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }

    if (errors.length === this.config.order.length) {
      throw new ContentFetchError(`every content origin failed for ${repoPath}`, {
        cause: errors[0],
      });
    }
    return null;
  }

  async readJson(filePath: string): Promise<{ data: unknown; file: SourceFile } | null> {
    const file = await this.read(filePath);
    if (!file) return null;
    try {
      return { data: JSON.parse(file.content) as unknown, file };
    } catch (error) {
      throw new ContentFetchError(`invalid JSON in ${file.path} (${file.origin})`, {
        cause: error,
      });
    }
  }

  private async readGitHub(repoPath: string): Promise<string | null> {
    const url = this.urls.raw(repoPath);
    const response = await this.fetchImpl(url, {
      headers: this.config.token ? { Authorization: `Bearer ${this.config.token}` } : undefined,
      next: { revalidate: this.config.revalidateSeconds, tags: [CONTENT_CACHE_TAG] },
    });
    if (response.status === 404) return null;
    if (!response.ok) {
      throw new ContentFetchError(`GET ${url} returned ${response.status}`);
    }
    return response.text();
  }

  private async readLocal(repoPath: string): Promise<string | null> {
    const fullPath = path.join(this.config.localRoot, ...repoPath.split("/"));
    try {
      return await readFile(fullPath, "utf8");
    } catch (error) {
      if (isMissingFile(error)) return null;
      throw error;
    }
  }
}
