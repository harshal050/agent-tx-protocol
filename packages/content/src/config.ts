import { existsSync } from "node:fs";
import path from "node:path";

/** Where content is read from. */
export type ContentSourceMode = "auto" | "github" | "local";
export type ContentOrigin = "github" | "local";

export interface ContentConfig {
  /** `owner/name` */
  repo: string;
  owner: string;
  name: string;
  branch: string;
  mode: ContentSourceMode;
  /** Origins tried in order. */
  order: ContentOrigin[];
  /** Optional GitHub token (raises rate limits). */
  token: string | undefined;
  /** Monorepo checkout used for local reads. */
  localRoot: string;
  /** Repository directory holding the documentation. */
  docsDir: string;
  /** Revalidation window for remote data, in seconds. */
  revalidateSeconds: number;
}

export type EnvLike = Record<string, string | undefined>;

export const DEFAULT_REPO = "harshal050/agent-tx-protocol";
export const DEFAULT_BRANCH = "main";
export const DOCS_DIR = "docs";
export const DEFAULT_REVALIDATE_SECONDS = 3600;

const MODES: readonly ContentSourceMode[] = ["auto", "github", "local"];

function isMode(value: string): value is ContentSourceMode {
  return (MODES as readonly string[]).includes(value);
}

/**
 * Resolves content configuration from environment variables.
 *
 * - `AGENTTX_GITHUB_REPO` (default `harshal050/agent-tx-protocol`)
 * - `AGENTTX_GITHUB_BRANCH` (default `main`)
 * - `AGENTTX_CONTENT_SOURCE`: `auto` (GitHub first in production, local first
 *   in development), `github` or `local`
 * - `AGENTTX_LOCAL_ROOT`: monorepo root (auto-detected)
 * - `GITHUB_TOKEN`: optional
 */
export function resolveContentConfig(
  env: EnvLike = process.env,
  cwd: string = process.cwd(),
): ContentConfig {
  const repo = env.AGENTTX_GITHUB_REPO?.trim() || DEFAULT_REPO;
  const match = /^([\w.-]+)\/([\w.-]+)$/.exec(repo);
  if (!match) {
    throw new Error(`AGENTTX_GITHUB_REPO must look like "owner/name", got "${repo}"`);
  }
  const branch = env.AGENTTX_GITHUB_BRANCH?.trim() || DEFAULT_BRANCH;
  if (!/^[\w./-]+$/.test(branch)) {
    throw new Error(`AGENTTX_GITHUB_BRANCH contains unsupported characters: "${branch}"`);
  }
  const mode = env.AGENTTX_CONTENT_SOURCE?.trim() || "auto";
  if (!isMode(mode)) {
    throw new Error(`AGENTTX_CONTENT_SOURCE must be one of ${MODES.join(", ")}, got "${mode}"`);
  }

  let order: ContentOrigin[];
  if (mode === "github") order = ["github"];
  else if (mode === "local") order = ["local"];
  else order = env.NODE_ENV === "development" ? ["local", "github"] : ["github", "local"];

  return {
    repo,
    owner: match[1]!,
    name: match[2]!,
    branch,
    mode,
    order,
    token: env.GITHUB_TOKEN?.trim() || undefined,
    localRoot: env.AGENTTX_LOCAL_ROOT
      ? path.resolve(cwd, env.AGENTTX_LOCAL_ROOT)
      : findRepoRoot(cwd),
    docsDir: DOCS_DIR,
    revalidateSeconds: DEFAULT_REVALIDATE_SECONDS,
  };
}

/** Walks up from `start` to the directory containing `docs/navigation.json`. */
export function findRepoRoot(start: string): string {
  let dir = path.resolve(start);
  for (;;) {
    if (existsSync(path.join(dir, DOCS_DIR, "navigation.json"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return path.resolve(start);
    dir = parent;
  }
}

function encodePath(repoPath: string): string {
  return repoPath.split("/").map(encodeURIComponent).join("/");
}

/** URL builders for the configured repository. */
export function repoUrls(config: Pick<ContentConfig, "repo" | "branch">) {
  const base = `https://github.com/${config.repo}`;
  return {
    repo: base,
    issues: `${base}/issues`,
    discussions: `${base}/discussions`,
    blob: (repoPath: string) => `${base}/blob/${config.branch}/${encodePath(repoPath)}`,
    edit: (repoPath: string) => `${base}/edit/${config.branch}/${encodePath(repoPath)}`,
    commits: (repoPath: string) => `${base}/commits/${config.branch}/${encodePath(repoPath)}`,
    raw: (repoPath: string) =>
      `https://raw.githubusercontent.com/${config.repo}/${config.branch}/${encodePath(repoPath)}`,
    api: `https://api.github.com/repos/${config.repo}`,
  };
}
