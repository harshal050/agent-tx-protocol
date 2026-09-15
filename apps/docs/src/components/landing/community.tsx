import { formatCount } from "@agenttx/benchmarks";
import type { CommitInfo, Contributor, LanguageShare, RepoStats } from "@agenttx/content";
import { buttonVariants } from "@agenttx/ui/button";
import { GitCommitHorizontal, GitFork, MessageSquareWarning, Star, Users } from "lucide-react";
import Image from "next/image";
import Link from "next/link";

import { GitHubIcon } from "@/components/brand";
import { SectionHeading } from "@/components/section-heading";

const LANGUAGE_COLORS = ["var(--chart-after)", "var(--chart-series-2)", "#1baf7a"];

const dateFormat = new Intl.DateTimeFormat("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });

export function Community({
  repoUrl,
  stats,
  contributors,
  languages,
  commit,
}: {
  repoUrl: string;
  stats: RepoStats | null;
  contributors: Contributor[];
  languages: LanguageShare[];
  commit: CommitInfo | null;
}) {
  const top = languages.slice(0, 3);
  const otherPercent = languages.slice(3).reduce((sum, l) => sum + l.percent, 0);
  const segments = [
    ...top.map((l, i) => ({ name: l.name, percent: l.percent, color: LANGUAGE_COLORS[i]! })),
    ...(otherPercent > 0.05 ? [{ name: "Other", percent: otherPercent, color: "var(--chart-before)" }] : []),
  ];

  const tiles = [
    { icon: Star, label: "Stars", value: stats ? formatCount(stats.stars) : "—" },
    { icon: GitFork, label: "Forks", value: stats ? formatCount(stats.forks) : "—" },
    { icon: MessageSquareWarning, label: "Open issues", value: stats ? formatCount(stats.openIssues) : "—" },
    { icon: Users, label: "Contributors", value: contributors.length ? formatCount(contributors.length) : "—" },
  ];

  return (
    <section className="mx-auto max-w-7xl px-4 py-24 sm:px-6 sm:py-28">
      <div className="grid gap-12 lg:grid-cols-[0.9fr_1.1fr]">
        <div>
          <SectionHeading
            eyebrow="Open source"
            title="Built in the open, on GitHub."
            description="The docs you are reading, the benchmark results and these numbers are loaded straight from the repository and refreshed on every push."
          />
          <div className="mt-8 flex flex-wrap gap-3">
            <a href={repoUrl} target="_blank" rel="noopener noreferrer" className={buttonVariants()}>
              <GitHubIcon /> Star on GitHub
            </a>
            <Link href="/docs/contributing" className={buttonVariants({ variant: "secondary" })}>
              Contribute
            </Link>
          </div>
        </div>

        <div className="rounded-2xl border border-border bg-surface p-6">
          <dl className="grid grid-cols-2 gap-4 sm:grid-cols-4">
            {tiles.map(({ icon: Icon, label, value }) => (
              <div key={label}>
                <dt className="flex items-center gap-1.5 text-[12.5px] text-fg-muted">
                  <Icon className="size-3.5" /> {label}
                </dt>
                <dd className="mt-1 text-2xl font-semibold tracking-tight text-fg">{value}</dd>
              </div>
            ))}
          </dl>

          {segments.length > 0 && (
            <div className="mt-7">
              <div className="flex h-2 gap-0.5 overflow-hidden rounded-full" role="img" aria-label={segments.map((s) => `${s.name} ${s.percent.toFixed(1)}%`).join(", ")}>
                {segments.map((segment) => (
                  <span key={segment.name} style={{ width: `${segment.percent}%`, background: segment.color }} />
                ))}
              </div>
              <ul className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-xs text-fg-muted">
                {segments.map((segment) => (
                  <li key={segment.name} className="flex items-center gap-1.5">
                    <span className="size-2 rounded-full" style={{ background: segment.color }} aria-hidden="true" />
                    {segment.name} <span className="text-fg-subtle tabular-nums">{segment.percent.toFixed(1)}%</span>
                  </li>
                ))}
              </ul>
            </div>
          )}

          {contributors.length > 0 && (
            <div className="mt-7 flex flex-wrap gap-2">
              {contributors.map((c) => (
                <a key={c.login} href={c.htmlUrl} target="_blank" rel="noopener noreferrer" title={`${c.login} · ${c.contributions} commits`}>
                  <Image
                    src={`${c.avatarUrl}${c.avatarUrl.includes("?") ? "&" : "?"}s=80`}
                    alt={c.login}
                    width={36}
                    height={36}
                    className="size-9 rounded-full border border-border transition-transform hover:-translate-y-0.5"
                  />
                </a>
              ))}
            </div>
          )}

          {commit && (
            <a
              href={commit.htmlUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="mt-7 flex items-center gap-3 rounded-xl border border-border bg-surface-2/60 px-4 py-3 text-[13px] transition-colors hover:border-border-strong"
            >
              <GitCommitHorizontal className="size-4 shrink-0 text-fg-subtle" />
              <span className="font-mono text-fg-muted">{commit.shortSha}</span>
              <span className="min-w-0 flex-1 truncate text-fg">{commit.message}</span>
              {commit.date && <span className="hidden shrink-0 text-fg-subtle sm:inline">{dateFormat.format(new Date(commit.date))}</span>}
            </a>
          )}
        </div>
      </div>
    </section>
  );
}
