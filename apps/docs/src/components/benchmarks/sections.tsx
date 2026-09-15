"use client";

import {
  formatBytes,
  formatCount,
  formatMicros,
  formatMillis,
  formatPercent,
  type CleanerResult,
  type CommitPoint,
  type LatencyResult,
  type RestorePoint,
  type ThroughputPoint,
} from "@agenttx/benchmarks";
import { ColumnChart } from "@agenttx/ui/charts/column-chart";
import { DumbbellChart } from "@agenttx/ui/charts/dumbbell-chart";
import { LineChart } from "@agenttx/ui/charts/line-chart";
import { ChartCard, Legend, chartColors } from "@agenttx/ui/charts/primitives";
import { RangeBarChart } from "@agenttx/ui/charts/range-bar-chart";
import { Badge, Callout, DataTable, StatTile } from "@agenttx/ui/primitives";
import { cn } from "@agenttx/ui/lib/cn";
import { useState } from "react";

import { changeLabel } from "./delta-badge";

const msFromUs = (us: number) => us / 1000;

export function LatencySection({ results, llmStepMs }: { results: LatencyResult[]; llmStepMs: number }) {
  const direct = results.find((r) => r.mode === "direct")?.summary;
  const engine = results.find((r) => r.mode === "engine")?.summary;
  const grpc = results.find((r) => r.mode === "grpc")?.summary;
  const llmUs = llmStepMs * 1000;

  return (
    <div className="space-y-4">
      <div className="grid gap-3 sm:grid-cols-3">
        {direct && <StatTile label="Direct tool call (before)" value={formatMicros(direct.p50_us)} detail={`p50 · p99 ${formatMicros(direct.p99_us)}`} />}
        {engine && direct && (
          <StatTile
            label="AgentTx engine, embedded (after)"
            value={formatMicros(engine.p50_us)}
            detail={`+${formatMicros(Math.max(0, engine.p50_us - direct.p50_us))} per step · ${formatPercent(((engine.p50_us - direct.p50_us) / llmUs) * 100, 3)} of a ${formatMillis(llmStepMs)} LLM call`}
          />
        )}
        {grpc && direct && (
          <StatTile
            label="AgentTx over gRPC (after)"
            value={formatMicros(grpc.p50_us)}
            detail={`+${formatMicros(Math.max(0, grpc.p50_us - direct.p50_us))} per step · ${formatPercent(((grpc.p50_us - direct.p50_us) / llmUs) * 100, 2)} of a ${formatMillis(llmStepMs)} LLM call`}
          />
        )}
      </div>
      <ChartCard
        title="Per-step latency: kv.put"
        description="Log scale. What a single tool call costs with and without the transaction layer. The added time buys snapshots, the undo journal, dependency tracking and rollback."
        legend={
          <Legend
            items={[
              { label: "p50 (bar)", color: chartColors.after, shape: "rect" },
              { label: "p95 tick · p99 whisker", color: chartColors.after, shape: "line" },
            ]}
          />
        }
        table={
          <DataTable
            caption="Per-step latency percentiles"
            columns={[
              { key: "mode", label: "Mode" },
              { key: "p50", label: "p50", align: "right" },
              { key: "p95", label: "p95", align: "right" },
              { key: "p99", label: "p99", align: "right" },
              { key: "samples", label: "Samples", align: "right" },
            ]}
            rows={results.map((r) => ({
              mode: r.label,
              p50: formatMicros(r.summary.p50_us),
              p95: formatMicros(r.summary.p95_us),
              p99: formatMicros(r.summary.p99_us),
              samples: formatCount(r.summary.samples),
            }))}
          />
        }
        footer={
          <ul className="space-y-1">
            {results.map((r) => (
              <li key={r.mode}>
                <span className="font-medium text-fg-muted">{r.label}:</span> {r.description}
              </li>
            ))}
          </ul>
        }
      >
        <RangeBarChart
          rows={results.map((r) => ({ id: r.mode, label: r.label, p50: r.summary.p50_us, p95: r.summary.p95_us, p99: r.summary.p99_us }))}
          format={formatMicros}
          ariaLabel="Per-step latency percentiles for direct calls, the embedded engine and gRPC"
        />
      </ChartCard>
    </div>
  );
}

export function RestoreSection({ restore, commit }: { restore: RestorePoint[]; commit: CommitPoint[] }) {
  const largest = restore.at(-1);
  return (
    <div className="grid gap-4 lg:grid-cols-[1.35fr_1fr]">
      <ChartCard
        title="Snapshot restore vs. the writes it undoes"
        description={
          largest
            ? `Rewinding ${formatCount(largest.journal_entries)} journaled writes takes ${formatMillis(msFromUs(largest.restore.p50_us))} (p50) — ${formatPercent((largest.restore.p50_us / largest.forward_writes.p50_us) * 100)} of the time the writes took. Log–log scale.`
            : undefined
        }
        legend={
          <Legend
            items={[
              { label: "Restore (p50)", color: chartColors.after, shape: "line" },
              { label: "Original writes (p50)", color: chartColors.series2, shape: "line" },
            ]}
          />
        }
        table={
          <DataTable
            caption="Snapshot restore latency"
            columns={[
              { key: "entries", label: "Journal entries", align: "right" },
              { key: "restore", label: "Restore p50", align: "right" },
              { key: "restore99", label: "Restore p99", align: "right" },
              { key: "writes", label: "Writes p50", align: "right" },
            ]}
            rows={restore.map((p) => ({
              entries: formatCount(p.journal_entries),
              restore: formatMillis(msFromUs(p.restore.p50_us)),
              restore99: formatMillis(msFromUs(p.restore.p99_us)),
              writes: formatMillis(msFromUs(p.forward_writes.p50_us)),
            }))}
          />
        }
      >
        <LineChart
          ariaLabel="Snapshot restore time and original write time by number of journal entries"
          xScale="log"
          yScale="log"
          formatX={(v) => formatCount(v)}
          formatY={(v) => formatMillis(v)}
          threshold={{ value: 50, label: "50 ms target" }}
          series={[
            { id: "restore", label: "Restore", color: chartColors.after, points: restore.map((p) => ({ x: p.journal_entries, y: msFromUs(p.restore.p50_us) })) },
            { id: "writes", label: "Writes", color: chartColors.series2, points: restore.map((p) => ({ x: p.journal_entries, y: msFromUs(p.forward_writes.p50_us) })) },
          ]}
        />
      </ChartCard>
      <ChartCard
        title="Commit latency"
        description="Atomic publish with write-write conflict checks and cleanup of journal, snapshots and Saga logs (p50)."
        table={
          <DataTable
            caption="Commit latency by keys"
            columns={[
              { key: "keys", label: "Keys", align: "right" },
              { key: "p50", label: "p50", align: "right" },
              { key: "p99", label: "p99", align: "right" },
            ]}
            rows={commit.map((c) => ({ keys: formatCount(c.keys), p50: formatMillis(msFromUs(c.commit.p50_us)), p99: formatMillis(msFromUs(c.commit.p99_us)) }))}
          />
        }
      >
        <ColumnChart
          ariaLabel="Commit latency by number of keys"
          seriesLabel="commit p50"
          format={(v) => formatMillis(v)}
          data={commit.map((c) => ({ id: String(c.keys), label: formatCount(c.keys), value: msFromUs(c.commit.p50_us) }))}
        />
      </ChartCard>
    </div>
  );
}

export function CleanerSection({ results }: { results: CleanerResult[] }) {
  const [selectedId, setSelectedId] = useState(results[0]?.id);
  const selected = results.find((r) => r.id === selectedId) ?? results[0];
  if (!selected) return null;

  return (
    <div className="space-y-4">
      <ChartCard
        title="Context size: raw error → Clean Hint"
        description="Log scale. Bytes an agent would add to its prompt for each real-world trace, before and after cleaning."
        legend={
          <Legend
            items={[
              { label: "Raw error (before)", color: chartColors.before, shape: "dot" },
              { label: "Clean Hint (after)", color: chartColors.after, shape: "dot" },
            ]}
          />
        }
        table={
          <DataTable
            caption="Error cleaner results"
            columns={[
              { key: "label", label: "Error" },
              { key: "runtime", label: "Runtime" },
              { key: "raw", label: "Raw", align: "right" },
              { key: "hint", label: "Hint", align: "right" },
              { key: "reduction", label: "Reduction", align: "right" },
              { key: "parse", label: "Parse p50", align: "right" },
            ]}
            rows={results.map((r) => ({
              label: r.label,
              runtime: r.runtime,
              raw: formatBytes(r.raw_bytes),
              hint: formatBytes(r.hint_bytes),
              reduction: formatPercent(r.reduction_pct),
              parse: formatMicros(r.parse.p50_us),
            }))}
          />
        }
      >
        <DumbbellChart
          scale="log"
          rows={results.map((r) => ({ id: r.id, label: r.runtime, before: r.raw_bytes, after: r.hint_bytes }))}
          format={formatBytes}
          beforeLabel="raw error"
          afterLabel="Clean Hint"
          annotate={(row) => changeLabel(row.before, row.after)}
          ariaLabel="Raw error size versus Clean Hint size per fixture"
        />
      </ChartCard>

      <div className="grid gap-4 rounded-2xl border border-border bg-surface p-3 lg:grid-cols-[260px_minmax(0,1fr)]">
        <div role="listbox" aria-label="Error fixtures" className="flex gap-1 overflow-x-auto lg:flex-col">
          {results.map((r) => (
            <button
              key={r.id}
              type="button"
              role="option"
              aria-selected={r.id === selected.id}
              onClick={() => setSelectedId(r.id)}
              className={cn(
                "shrink-0 rounded-lg px-3 py-2.5 text-left transition-colors",
                r.id === selected.id ? "bg-surface-2" : "hover:bg-surface-2/60",
              )}
            >
              <div className="text-[13.5px] font-medium text-fg">{r.label}</div>
              <div className="mt-0.5 text-xs text-fg-subtle">
                {r.runtime} · {formatBytes(r.raw_bytes)} → {formatBytes(r.hint_bytes)}
              </div>
            </button>
          ))}
        </div>
        <div className="min-w-0 space-y-3 p-2">
          <div className="flex flex-wrap items-center gap-2">
            <Badge>{selected.runtime}</Badge>
            <Badge tone={selected.rule ? "accent" : "neutral"}>{selected.rule ?? "fallback"}</Badge>
            <Badge>{selected.category.replaceAll("_", " ")}</Badge>
            {selected.key && <Badge>key: {selected.key}</Badge>}
            <span className="text-xs text-fg-subtle">parsed in {formatMicros(selected.parse.p50_us)} (p50)</span>
          </div>
          <div>
            <div className="mb-1.5 text-xs font-medium text-fg-subtle">
              Before — {formatBytes(selected.raw_bytes)}, {selected.raw_lines} lines
            </div>
            <pre className="max-h-64 overflow-auto rounded-xl border border-border bg-surface-2 p-3.5 font-mono text-[11.5px] leading-relaxed whitespace-pre text-fg-muted">
              {selected.raw}
            </pre>
          </div>
          <Callout kind="tip" title={`After — ${formatBytes(selected.hint_bytes)}`}>
            <code className="font-mono text-[13px] text-fg">{selected.hint}</code>
          </Callout>
        </div>
      </div>
    </div>
  );
}

export function ThroughputSection({ points }: { points: ThroughputPoint[] }) {
  const best = [...points].sort((a, b) => b.steps_per_sec - a.steps_per_sec)[0];
  return (
    <ChartCard
      title="Throughput: concurrent transactions"
      description={
        best
          ? `In-process engine, ${points[0]?.steps_per_transaction ?? 20} steps + commit per transaction. Peak ${formatCount(Math.round(best.steps_per_sec))} steps/s at concurrency ${best.concurrency}.`
          : undefined
      }
      table={
        <DataTable
          caption="Throughput by concurrency"
          columns={[
            { key: "concurrency", label: "Concurrency", align: "right" },
            { key: "tx", label: "Transactions", align: "right" },
            { key: "txps", label: "Tx / s", align: "right" },
            { key: "sps", label: "Steps / s", align: "right" },
          ]}
          rows={points.map((p) => ({
            concurrency: p.concurrency,
            tx: formatCount(p.transactions),
            txps: formatCount(Math.round(p.transactions_per_sec)),
            sps: formatCount(Math.round(p.steps_per_sec)),
          }))}
        />
      }
    >
      <ColumnChart
        ariaLabel="Steps per second by concurrency"
        seriesLabel="steps / s"
        format={(v) => formatCount(Math.round(v))}
        data={points.map((p) => ({ id: String(p.concurrency), label: `×${p.concurrency}`, value: p.steps_per_sec }))}
      />
    </ChartCard>
  );
}
