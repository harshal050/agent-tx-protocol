import { z } from "zod";

import type { ContentConfig } from "./config";
import type { FetchLike } from "./source";

/** Cache tag attached to every GitHub API request. */
export const GITHUB_CACHE_TAG = "agenttx:github";

const repoSchema = z.object({
  full_name: z.string(),
  description: z.string().nullable(),
  html_url: z.string(),
  stargazers_count: z.number(),
  forks_count: z.number(),
  open_issues_count: z.number(),
  subscribers_count: z.number().optional(),
  license: z.object({ spdx_id: z.string().nullable(), name: z.string() }).nullable(),
  default_branch: z.string(),
  pushed_at: z.string(),
  topics: z.array(z.string()).optional(),
});

const contributorsSchema = z.array(
  z.object({
    login: z.string(),
    avatar_url: z.string(),
    html_url: z.string(),
    contributions: z.number(),
    type: z.string(),
  }),
);

const commitsSchema = z.array(
  z.object({
    sha: z.string(),
    html_url: z.string(),
    commit: z.object({
      message: z.string(),
      author: z.object({ name: z.string(), date: z.string() }).nullable(),
    }),
    author: z.object({ login: z.string(), avatar_url: z.string() }).nullable(),
  }),
);

const releaseSchema = z.object({
  tag_name: z.string(),
  name: z.string().nullable(),
  published_at: z.string().nullable(),
  html_url: z.string(),
});

const languagesSchema = z.record(z.string(), z.number());

export interface RepoStats {
  fullName: string;
  description: string | null;
  htmlUrl: string;
  stars: number;
  forks: number;
  openIssues: number;
  watchers: number;
  license: string | null;
  defaultBranch: string;
  pushedAt: string;
  topics: string[];
}

export interface Contributor {
  login: string;
  avatarUrl: string;
  htmlUrl: string;
  contributions: number;
}

export interface CommitInfo {
  sha: string;
  shortSha: string;
  message: string;
  author: string | null;
  authorLogin: string | null;
  date: string | null;
  htmlUrl: string;
}

export interface ReleaseInfo {
  tag: string;
  name: string;
  publishedAt: string | null;
  htmlUrl: string;
}

export interface LanguageShare {
  name: string;
  bytes: number;
  percent: number;
}

/**
 * Minimal, failure-tolerant GitHub REST client. Every method resolves to
 * `null` (or an empty list) when GitHub is unreachable or rate-limited, so
 * pages degrade gracefully instead of failing to render.
 */
export class GitHubClient {
  private readonly config: ContentConfig;
  private readonly fetchImpl: FetchLike;

  constructor(config: ContentConfig, fetchImpl?: FetchLike) {
    this.config = config;
    this.fetchImpl = fetchImpl ?? ((input, init) => fetch(input, init));
  }

  private async get<T>(pathAndQuery: string, schema: z.ZodType<T>): Promise<T | null> {
    const url = `https://api.github.com/repos/${this.config.repo}${pathAndQuery}`;
    try {
      const response = await this.fetchImpl(url, {
        headers: {
          Accept: "application/vnd.github+json",
          "X-GitHub-Api-Version": "2022-11-28",
          ...(this.config.token ? { Authorization: `Bearer ${this.config.token}` } : {}),
        },
        next: { revalidate: this.config.revalidateSeconds, tags: [GITHUB_CACHE_TAG] },
      });
      if (!response.ok) {
        if (response.status !== 404) {
          console.warn(`[github] GET ${url} returned ${response.status}`);
        }
        return null;
      }
      const parsed = schema.safeParse(await response.json());
      if (!parsed.success) {
        console.warn(`[github] unexpected response shape for ${url}`);
        return null;
      }
      return parsed.data;
    } catch (error) {
      console.warn(
        `[github] GET ${url} failed: ${error instanceof Error ? error.message : String(error)}`,
      );
      return null;
    }
  }

  async repository(): Promise<RepoStats | null> {
    const repo = await this.get("", repoSchema);
    if (!repo) return null;
    return {
      fullName: repo.full_name,
      description: repo.description,
      htmlUrl: repo.html_url,
      stars: repo.stargazers_count,
      forks: repo.forks_count,
      openIssues: repo.open_issues_count,
      watchers: repo.subscribers_count ?? 0,
      license: repo.license?.spdx_id && repo.license.spdx_id !== "NOASSERTION" ? repo.license.spdx_id : null,
      defaultBranch: repo.default_branch,
      pushedAt: repo.pushed_at,
      topics: repo.topics ?? [],
    };
  }

  async contributors(limit = 24): Promise<Contributor[]> {
    const list = await this.get(`/contributors?per_page=${Math.min(limit, 100)}`, contributorsSchema);
    return (list ?? [])
      .filter((c) => c.type !== "Bot")
      .map((c) => ({
        login: c.login,
        avatarUrl: c.avatar_url,
        htmlUrl: c.html_url,
        contributions: c.contributions,
      }));
  }

  /** Latest commit on the configured branch, optionally touching `filePath`. */
  async latestCommit(filePath?: string): Promise<CommitInfo | null> {
    const params = new URLSearchParams({ per_page: "1", sha: this.config.branch });
    if (filePath) params.set("path", filePath);
    const commits = await this.get(`/commits?${params.toString()}`, commitsSchema);
    const commit = commits?.[0];
    if (!commit) return null;
    return {
      sha: commit.sha,
      shortSha: commit.sha.slice(0, 7),
      message: commit.commit.message.split("\n")[0] ?? "",
      author: commit.commit.author?.name ?? null,
      authorLogin: commit.author?.login ?? null,
      date: commit.commit.author?.date ?? null,
      htmlUrl: commit.html_url,
    };
  }

  async latestRelease(): Promise<ReleaseInfo | null> {
    const release = await this.get("/releases/latest", releaseSchema);
    if (!release) return null;
    return {
      tag: release.tag_name,
      name: release.name ?? release.tag_name,
      publishedAt: release.published_at,
      htmlUrl: release.html_url,
    };
  }

  async languages(): Promise<LanguageShare[]> {
    const languages = await this.get("/languages", languagesSchema);
    if (!languages) return [];
    const total = Object.values(languages).reduce((sum, bytes) => sum + bytes, 0);
    if (total === 0) return [];
    return Object.entries(languages)
      .map(([name, bytes]) => ({ name, bytes, percent: (bytes / total) * 100 }))
      .sort((a, b) => b.bytes - a.bytes);
  }
}
