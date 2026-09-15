import { formatDate, formatMillis } from "@agenttx/benchmarks";
import { CopyButton } from "@agenttx/ui/interactive";
import { Badge } from "@agenttx/ui/primitives";
import { ArrowUpRight, Cpu, FileText, GitCommitHorizontal, Terminal } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";

import { RecoveryExplorer } from "@/components/benchmarks/recovery-explorer";
import { CleanerSection, LatencySection, RestoreSection, ThroughputSection } from "@/components/benchmarks/sections";
import { SectionHeading } from "@/components/section-heading";
import { getBenchmarkReport } from "@/lib/benchmarks";
import { repoLinks } from "@/lib/content";

export const revalidate = 3600;

export const metadata: Metadata = {
  title: "Benchmarks",
  description:
    "Measured before/after results: tool calls, context tokens, side effects, per-step latency, snapshot restore, error cleaning and throughput for AgentTx.",
  alternates: { canonical: "/benchmarks" },
};

const REPRODUCE = "cargo run --release -p agenttx-bench -- --out benchmarks/results/latest.json";

const SECTIONS = [
  { id: "recovery", label: "Agent recovery" },
  { id: "latency", label: "Step latency" },
  { id: "rewind", label: "Rewind & commit" },
  { id: "cleaner", label: "Error cleaner" },
  { id: "throughput", label: "Throughput" },
  { id: "methodology", label: "Methodology" },
];

function Command({ value }: { value: string }) {
  return (
    <div className="flex w-full max-w-full items-center gap-3 rounded-xl border border-border bg-surface/80 py-1.5 pr-1.5 pl-4 font-mono text-[12.5px] backdrop-blur sm:w-fit">
      <Terminal className="size-4 shrink-0 text-fg-subtle" />
      <code className="min-w-0 truncate text-fg">{value}</code>
      <CopyButton value={value} label="Copy command" />
    </div>
  );
}

export default async function BenchmarksPage() {
  const loaded = await getBenchmarkReport();

  if (!loaded) {
    return (
      <main className="mx-auto max-w-3xl px-4 py-24 text-center sm:px-6">
        <h1 className="text-3xl font-semibold tracking-tight text-fg">No benchmark results yet</h1>
        <p className="mt-4 text-fg-muted">
          Run the harness to generate <code className="font-mono">benchmarks/results/latest.json</code>, then reload.
        </p>
        <div className="mt-8 flex justify-center">
          <Command value={REPRODUCE} />
        </div>
      </main>
    );
  }

  const { report, file } = loaded;
  const env = report.environment;
  const shortSha = env.git_sha?.slice(0, 7);

  return (
    <main>
      <section className="relative overflow-hidden border-b border-border">
        <div aria-hidden="true" className="bg-grid absolute inset-0" />
        <div aria-hidden="true" className="hero-glow absolute inset-0" />
        <div className="relative mx-auto max-w-7xl px-4 pt-14 pb-12 sm:px-6 sm:pt-20 sm:pb-16">
          <p className="text-[12.5px] font-semibold tracking-[0.1em] text-accent-ink uppercase">Benchmarks</p>
          <h1 className="mt-3 max-w-3xl text-4xl leading-[1.05] font-semibold tracking-[-0.04em] text-balance text-fg sm:text-6xl">
            Before and after AgentTx, measured.
          </h1>
          <p className="mt-5 max-w-2xl text-[17px] leading-relaxed text-pretty text-fg-muted">
            Every chart renders the committed results file. The same scripted agent and the same tools run with and
            without AgentTx; nothing on this page is typed in by hand.
          </p>

          <div className="mt-7 flex flex-wrap items-center gap-2">
            <Badge>
              <Cpu /> {env.cpu_model ?? env.arch} · {env.logical_cores} cores{env.memory_gb ? ` · ${env.memory_gb} GB` : ""}
            </Badge>
            {env.rustc && <Badge>{env.rustc.split(" (")[0]}</Badge>}
            <Badge>{env.build_profile} build</Badge>
            {shortSha && (
              <a href={`${repoLinks.repo}/commit/${env.git_sha}`} target="_blank" rel="noopener noreferrer">
                <Badge tone={env.git_dirty ? "warning" : "neutral"}>
                  <GitCommitHorizontal />
                  {shortSha}
                  {env.git_dirty ? " + uncommitted changes" : ""}
                </Badge>
              </a>
            )}
            <Badge>{formatDate(report.generated_at_ms)}</Badge>
            <a
              href={file.htmlUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-xs font-medium text-fg-muted hover:text-fg"
            >
              <FileText className="size-3.5" /> Raw JSON <ArrowUpRight className="size-3" />
            </a>
          </div>

          <div className="mt-6">
            <Command value={REPRODUCE} />
          </div>
        </div>
      </section>

      <nav aria-label="Benchmark sections" className="sticky top-14 z-30 border-b border-border bg-bg/80 backdrop-blur-xl">
        <div className="mx-auto flex max-w-7xl gap-1 overflow-x-auto px-4 py-2 sm:px-6">
          {SECTIONS.map((section) => (
            <a
              key={section.id}
              href={`#${section.id}`}
              className="shrink-0 rounded-md px-3 py-1.5 text-[13px] font-medium whitespace-nowrap text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg"
            >
              {section.label}
            </a>
          ))}
        </div>
      </nav>

      <div className="mx-auto max-w-7xl space-y-24 px-4 py-16 sm:px-6 sm:py-20">
        <section aria-labelledby="recovery">
          <SectionHeading
            id="recovery"
            eyebrow="1 · Agent recovery"
            title="What happens after a step fails."
            description="A naive loop retries the failing step, then restarts from scratch with raw errors in context. AgentTx rewinds to the right step with a one-line hint. Pick a scenario and size."
          />
          <div className="mt-8">
            <RecoveryExplorer scenarios={report.simulation} llmStepMs={report.config.llm_step_ms} />
          </div>
        </section>

        <section aria-labelledby="latency">
          <SectionHeading
            id="latency"
            eyebrow="2 · Step latency"
            title="The price of a safety net, per step."
            description={`Microseconds of overhead per tool call, next to an assumed ${formatMillis(report.config.llm_step_ms)} LLM round-trip that dominates real agent latency.`}
          />
          <div className="mt-8">
            <LatencySection results={report.step_latency} llmStepMs={report.config.llm_step_ms} />
          </div>
        </section>

        <section aria-labelledby="rewind">
          <SectionHeading
            id="rewind"
            eyebrow="3 · Rewind & commit"
            title="Rewinds scale with changes, not database size."
            description="The undo journal replays prior values in one atomic batch. Commit publishes the overlay and validates write-write conflicts."
          />
          <div className="mt-8">
            <RestoreSection restore={report.snapshot_restore} commit={report.commit} />
          </div>
        </section>

        <section aria-labelledby="cleaner">
          <SectionHeading
            id="cleaner"
            eyebrow="4 · Error cleaner"
            title="Kilobytes of stack trace in, one line out."
            description="Real traces from Java, Python, Node.js, Go and Rust services, reduced deterministically in microseconds. Select one to compare."
          />
          <div className="mt-8">
            <CleanerSection results={report.error_cleaner} />
          </div>
        </section>

        <section aria-labelledby="throughput">
          <SectionHeading
            id="throughput"
            eyebrow="5 · Throughput"
            title="Parallel transactions, serialized steps."
            description="Each transaction holds its own lock, so independent agents scale across cores."
          />
          <div className="mt-8">
            <ThroughputSection points={report.throughput} />
          </div>
        </section>

        <section aria-labelledby="methodology">
          <SectionHeading
            id="methodology"
            eyebrow="Methodology"
            title="How these numbers were produced."
            description={
              <>
                Harness version {report.harness_version}
                {report.config.quick ? " (quick mode)" : ""}. Full details in the{" "}
                <Link href="/docs/benchmark-methodology" className="font-medium text-fg underline decoration-accent/40 underline-offset-4 hover:decoration-accent">
                  benchmark methodology guide
                </Link>
                .
              </>
            }
          />
          <dl className="mt-8 grid gap-3 md:grid-cols-2">
            {[
              ["Agent model", report.methodology.agent_model],
              ["Without AgentTx (baseline)", report.methodology.baseline_policy],
              ["With AgentTx", report.methodology.agenttx_policy],
              ["Tokens", report.methodology.token_estimate],
              ["Modeled latency", report.methodology.modeled_latency],
            ].map(([term, detail]) => (
              <div key={term} className="rounded-2xl border border-border bg-surface p-5">
                <dt className="text-[13px] font-semibold text-fg">{term}</dt>
                <dd className="mt-2 text-[14.5px] leading-relaxed text-fg-muted">{detail}</dd>
              </div>
            ))}
          </dl>
        </section>
      </div>
    </main>
  );
}
