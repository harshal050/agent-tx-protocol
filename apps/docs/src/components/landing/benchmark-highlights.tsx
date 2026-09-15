import {
  compare,
  formatCount,
  formatDate,
  formatMicros,
  headline,
  type BenchmarkReport,
} from "@agenttx/benchmarks";
import { buttonVariants } from "@agenttx/ui/button";
import { StatTile } from "@agenttx/ui/primitives";
import { ArrowRight, Cpu } from "lucide-react";
import Link from "next/link";

import { DeltaBadge } from "@/components/benchmarks/delta-badge";
import { SectionHeading } from "@/components/section-heading";

import { HighlightsChart } from "./highlights-chart";

export function BenchmarkHighlights({ report }: { report: BenchmarkReport }) {
  const h = headline(report);
  const env = report.environment;
  const rootCause = report.simulation
    .filter((s) => s.kind === "root-cause")
    .sort((a, b) => a.steps - b.steps);

  return (
    <section className="border-y border-border bg-surface/50">
      <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 sm:py-28">
        <div className="flex flex-wrap items-end justify-between gap-6">
          <SectionHeading
            eyebrow="Benchmarks"
            title="Before and after, measured."
            description="Same scripted agent, same tools, with and without AgentTx. Every number below comes from the committed benchmark run — reproducible with one command."
          />
          <Link href="/benchmarks" className={buttonVariants({ variant: "secondary" })}>
            All benchmarks <ArrowRight />
          </Link>
        </div>

        <div className="mt-12 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {h.toolCalls && h.scenario && (
            <StatTile
              label={`Tool calls to recover · ${h.scenario.steps}-step task`}
              value={formatCount(h.toolCalls.after)}
              delta={<DeltaBadge comparison={h.toolCalls} />}
              detail={`vs ${formatCount(h.toolCalls.before)} without AgentTx`}
            />
          )}
          {h.feedbackTokens && (
            <StatTile
              label="Error tokens added to the prompt"
              value={formatCount(h.feedbackTokens.after)}
              delta={<DeltaBadge comparison={h.feedbackTokens} />}
              detail={`vs ${formatCount(h.feedbackTokens.before)} raw stack-trace tokens`}
            />
          )}
          <StatTile
            label="Side effects left wrong · all scenarios"
            value={formatCount(h.sideEffectDamage.after)}
            delta={<DeltaBadge comparison={h.sideEffectDamage} />}
            detail={`vs ${formatCount(h.sideEffectDamage.before)} duplicate, leaked or orphaned`}
          />
          {h.restore && (
            <StatTile
              label={`Snapshot restore · ${formatCount(h.restore.entries)} writes`}
              value={formatMicros(h.restore.p50Us)}
              detail={
                h.engineP50Us !== null
                  ? `p50 · engine step overhead ${formatMicros(h.engineP50Us)}`
                  : "p50"
              }
            />
          )}
        </div>

        {rootCause.length > 1 && (
          <div className="mt-4">
            <HighlightsChart
              rows={rootCause.map((s) => ({
                id: s.id,
                label: `${s.steps} steps`,
                before: s.baseline.tool_executions,
                after: s.agenttx.tool_executions,
              }))}
              summary={compare(
                rootCause.reduce((sum, s) => sum + s.baseline.tool_executions, 0),
                rootCause.reduce((sum, s) => sum + s.agenttx.tool_executions, 0),
              )}
            />
          </div>
        )}

        <p className="mt-5 flex flex-wrap items-center gap-2 text-xs text-fg-subtle">
          <Cpu className="size-3.5" />
          {env.cpu_model ?? env.arch} · {env.logical_cores} cores · {env.build_profile} build · {formatDate(report.generated_at_ms)}
        </p>
      </div>
    </section>
  );
}
