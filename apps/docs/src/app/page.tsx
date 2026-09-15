import { formatBytes, formatCount, formatMicros } from "@agenttx/benchmarks";

import { BenchmarkHighlights } from "@/components/landing/benchmark-highlights";
import { CodeShowcase } from "@/components/landing/code-showcase";
import { Community } from "@/components/landing/community";
import { Architecture, FinalCta, Features, Hero, HowItWorks, Problem } from "@/components/landing/sections";
import { getBenchmarkReport } from "@/lib/benchmarks";
import { getContributors, getLanguages, getLatestCommit, getRepoStats, repoLinks } from "@/lib/content";

export const revalidate = 3600;

export default async function HomePage() {
  const [stats, commit, contributors, languages, bench] = await Promise.all([
    getRepoStats(),
    getLatestCommit(),
    getContributors(),
    getLanguages(),
    getBenchmarkReport(),
  ]);

  const report = bench?.report;
  const smallRestore = report?.snapshot_restore.find((p) => p.journal_entries === 100) ?? report?.snapshot_restore[0];
  const trace = report?.error_cleaner.find((c) => c.id === "java-postgres-fk");

  return (
    <main>
      <Hero
        repoUrl={repoLinks.repo}
        stars={stats ? formatCount(stats.stars) : null}
        commit={commit}
        restoreLabel={smallRestore ? formatMicros(smallRestore.restore.p50_us) : undefined}
        traceLabel={trace ? formatBytes(trace.raw_bytes) : undefined}
      />
      <Problem />
      <HowItWorks />
      {report && <BenchmarkHighlights report={report} />}
      <Features />
      <CodeShowcase />
      <Architecture />
      <Community repoUrl={repoLinks.repo} stats={stats} contributors={contributors} languages={languages} commit={commit} />
      <FinalCta repoUrl={repoLinks.repo} />
    </main>
  );
}
