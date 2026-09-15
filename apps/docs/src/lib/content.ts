import "server-only";

import {
  GitHubClient,
  createDocsRepository,
  highlightCode,
  repoUrls,
  resolveContentConfig,
} from "@agenttx/content";
import { cache } from "react";

/** Resolved once per server process from AGENTTX_* environment variables. */
export const contentConfig = resolveContentConfig();
export const repoLinks = repoUrls(contentConfig);

const docs = createDocsRepository(contentConfig);
const github = new GitHubClient(contentConfig);

export const contentSource = docs.source;

export const getNavigation = cache(() => docs.navigation());

export const getDocPage = cache(async (slug: string) => docs.page(slug, await getNavigation()));

export const getSearchEntries = cache(async () => docs.searchEntries(await getNavigation()));

export const getRepoStats = cache(() => github.repository());
export const getContributors = cache(() => github.contributors(24));
export const getLatestCommit = cache(() => github.latestCommit());
export const getLanguages = cache(() => github.languages());

/**
 * Last commit touching a file. Only requested when a GitHub token is
 * configured: unauthenticated builds would exhaust the 60 requests/hour limit.
 */
export const getFileLastCommit = cache(async (filePath: string) =>
  contentConfig.token ? github.latestCommit(filePath) : null,
);

export const highlight = cache((code: string, lang: string) => highlightCode(code, lang));
