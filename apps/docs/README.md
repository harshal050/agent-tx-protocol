# @agenttx/docs

The AgentTx website — live at **[agenttx.site](https://agenttx.site)** — landing page, documentation and benchmark results. Built with Next.js (App Router), Tailwind CSS v4 and the shared `@agenttx/*` packages.

Content is **not** stored in this app:

| Data | Source |
|---|---|
| Documentation pages and navigation | `docs/*.md`, `docs/navigation.json` |
| Benchmark results | `benchmarks/results/latest.json` |
| Stars, contributors, languages, latest commit | GitHub REST API |

Pages are statically generated and revalidated hourly. A GitHub push webhook refreshes them immediately.

## Development

From the repository root:

```bash
pnpm install
pnpm dev   # http://localhost:3000
```

In development the site reads `docs/` and `benchmarks/` from your checkout first, so edits show up immediately.

## Environment

| Variable | Default | Purpose |
|---|---|---|
| `AGENTTX_GITHUB_REPO` | `harshal050/agent-tx-protocol` | Repository to read content from |
| `AGENTTX_GITHUB_BRANCH` | `main` | Branch to read |
| `AGENTTX_CONTENT_SOURCE` | `auto` | `auto` (GitHub first in production, local first in dev), `github`, or `local` |
| `GITHUB_TOKEN` | — | Optional. Raises API limits and enables per-page "Updated" dates |
| `GITHUB_WEBHOOK_SECRET` | — | Enables `POST /api/revalidate` |
| `NEXT_PUBLIC_SITE_URL` | `https://agenttx.site` | Canonical URL for metadata and the sitemap |

## Deploying on Vercel

1. Import the repository and set **Root Directory** to `apps/docs`. Vercel detects pnpm and Turborepo.
2. Keep "Include files outside the root directory" enabled, so `docs/` and `benchmarks/` can serve as the fallback.
3. Set `NEXT_PUBLIC_SITE_URL`, and optionally `GITHUB_TOKEN` and `GITHUB_WEBHOOK_SECRET`.
4. On GitHub, add a webhook: payload URL `https://<your-domain>/api/revalidate`, content type `application/json`, the same secret, and the **push** event.

Any Node.js host works too: `pnpm turbo run build --filter=@agenttx/docs`, then `pnpm --filter @agenttx/docs start`.
