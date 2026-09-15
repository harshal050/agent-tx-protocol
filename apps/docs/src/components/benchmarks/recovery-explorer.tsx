"use client";

import {
  METRICS,
  compareScenario,
  formatCount,
  formatMillis,
  metricValue,
  sideEffectDamage,
  type MetricId,
  type RunMetrics,
  type Scenario,
  type ScenarioKind,
} from "@agenttx/benchmarks";
import { DumbbellChart } from "@agenttx/ui/charts/dumbbell-chart";
import { ChartCard, Legend, chartColors } from "@agenttx/ui/charts/primitives";
import { SegmentedControl } from "@agenttx/ui/interactive";
import { Badge, DataTable, StatTile } from "@agenttx/ui/primitives";
import { CircleCheck, CircleX } from "lucide-react";
import { useState } from "react";

import { DeltaBadge, changeLabel } from "./delta-badge";

const METRIC_IDS = Object.keys(METRICS) as MetricId[];

function formatMetric(metric: MetricId, value: number): string {
  return METRICS[metric].unit === "ms" ? formatMillis(value) : formatCount(value);
}

function ActionChips({ run }: { run: RunMetrics }) {
  const entries = Object.entries(run.actions);
  if (entries.length === 0) return <span className="text-xs text-fg-subtle">No recovery actions needed</span>;
  return (
    <div className="flex flex-wrap gap-1.5">
      {entries.map(([action, count]) => (
        <span key={action} className="rounded-md border border-border bg-surface-2 px-2 py-0.5 font-mono text-[11.5px] text-fg-muted">
          {action.replaceAll("_", " ")} ×{count}
        </span>
      ))}
    </div>
  );
}

function RunPanel({ title, run, steps }: { title: string; run: RunMetrics; steps: number }) {
  const [expanded, setExpanded] = useState(false);
  const events = expanded ? run.events : run.events.slice(0, 8);
  const damage = sideEffectDamage(run);
  return (
    <div className="flex min-w-0 flex-col rounded-2xl border border-border bg-surface p-5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h4 className="font-semibold text-fg">{title}</h4>
        <Badge tone={run.succeeded ? "success" : "danger"}>
          {run.succeeded ? <CircleCheck /> : <CircleX />}
          {run.outcome}
        </Badge>
      </div>
      <dl className="mt-4 grid grid-cols-2 gap-x-4 gap-y-3 text-sm sm:grid-cols-4">
        {[
          ["Tool calls", `${formatCount(run.tool_executions)}`, `${steps} planned`],
          ["Failed calls", formatCount(run.failed_executions), ""],
          ["Feedback", `${formatCount(run.context_feedback_tokens_est)} tok`, `${formatCount(run.context_feedback_bytes)} B`],
          ["Left wrong", formatCount(damage), `${run.duplicate_side_effects} dup · ${run.leaked_side_effects} leaked · ${run.orphaned_files} files`],
        ].map(([label, value, hint]) => (
          <div key={label} className="min-w-0">
            <dt className="text-xs text-fg-subtle">{label}</dt>
            <dd className="mt-0.5 font-semibold text-fg tabular-nums">{value}</dd>
            {hint && <dd className="truncate text-[11px] text-fg-subtle">{hint}</dd>}
          </div>
        ))}
      </dl>
      <div className="mt-4">
        <ActionChips run={run} />
      </div>
      {run.events.length > 0 && (
        <ol className="mt-4 space-y-1 border-t border-border pt-3 font-mono text-[12px] text-fg-muted">
          {events.map((event, index) => (
            <li key={index} className="flex gap-2">
              <span className="w-5 shrink-0 text-right text-fg-subtle tabular-nums">{index + 1}</span>
              <span className="min-w-0 break-words">{event}</span>
            </li>
          ))}
        </ol>
      )}
      {run.events.length > 8 && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="mt-2 self-start text-xs font-medium text-fg-muted underline underline-offset-2 hover:text-fg"
        >
          {expanded ? "Show fewer events" : `Show all ${run.events.length} events`}
        </button>
      )}
    </div>
  );
}

export function RecoveryExplorer({ scenarios, llmStepMs }: { scenarios: Scenario[]; llmStepMs: number }) {
  const kinds = [...new Map(scenarios.map((s) => [s.kind, s])).values()];
  const [kind, setKind] = useState<ScenarioKind>(kinds[0]?.kind ?? "root-cause");
  const [metric, setMetric] = useState<MetricId>("tool_executions");
  const ofKind = scenarios.filter((s) => s.kind === kind).sort((a, b) => a.steps - b.steps);
  const [steps, setSteps] = useState<number>(ofKind.findLast((s) => s.steps <= 32)?.steps ?? ofKind[0]?.steps ?? 0);

  const selected = ofKind.find((s) => s.steps === steps) ?? ofKind.at(-1);
  if (!selected) return null;

  const changeKind = (next: ScenarioKind) => {
    setKind(next);
    const sizes = scenarios.filter((s) => s.kind === next).map((s) => s.steps);
    if (!sizes.includes(steps)) setSteps(sizes.filter((n) => n <= 32).at(-1) ?? sizes[0] ?? steps);
  };

  const rows = ofKind.map((s) => ({
    id: s.id,
    label: `${s.steps} steps`,
    before: metricValue(s.baseline, metric),
    after: metricValue(s.agenttx, metric),
  }));

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center gap-3">
        <SegmentedControl
          ariaLabel="Failure scenario"
          value={kind}
          onChange={changeKind}
          options={kinds.map((s) => ({ value: s.kind, label: s.title }))}
        />
        <SegmentedControl
          ariaLabel="Workflow size"
          value={String(selected.steps)}
          onChange={(value) => setSteps(Number(value))}
          options={ofKind.map((s) => ({ value: String(s.steps), label: `${s.steps} steps` }))}
        />
      </div>

      <p className="max-w-3xl text-[15px] leading-relaxed text-fg-muted">
        {selected.description}{" "}
        <span className="text-fg-subtle">
          {selected.root_step ? `Bad data at step ${selected.root_step}; ` : ""}failure at step {selected.failing_step} of {selected.steps}.
        </span>
      </p>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        {METRIC_IDS.map((id) => {
          const comparison = compareScenario(selected, id);
          return (
            <StatTile
              key={id}
              label={METRICS[id].label}
              value={formatMetric(id, comparison.after)}
              delta={<DeltaBadge comparison={comparison} />}
              detail={`vs ${formatMetric(id, comparison.before)} without AgentTx`}
            />
          );
        })}
      </div>

      <ChartCard
        title={`${METRICS[metric].label} by workflow size`}
        description={
          metric === "modeled_latency_ms"
            ? `Modeled: tool calls × ${formatMillis(llmStepMs)} assumed LLM latency per call + measured harness time.`
            : metric === "side_effect_damage"
              ? "Duplicate emails, effects sent by failed runs, and orphaned files left in the sandbox."
              : "Lower is better. Each row runs the same agent and tools twice."
        }
        legend={
          <div className="flex flex-wrap items-center justify-between gap-3">
            <Legend
              items={[
                { label: "Without AgentTx", color: chartColors.before, shape: "dot" },
                { label: "With AgentTx", color: chartColors.after, shape: "dot" },
              ]}
            />
            <SegmentedControl
              ariaLabel="Metric"
              value={metric}
              onChange={setMetric}
              options={METRIC_IDS.map((id) => ({ value: id, label: METRICS[id].label }))}
            />
          </div>
        }
        table={
          <DataTable
            caption={`${METRICS[metric].label} by workflow size`}
            columns={[
              { key: "size", label: "Workflow" },
              { key: "before", label: "Without AgentTx", align: "right" },
              { key: "after", label: "With AgentTx", align: "right" },
              { key: "change", label: "Change", align: "right" },
            ]}
            rows={rows.map((row) => ({
              size: row.label,
              before: formatMetric(metric, row.before),
              after: formatMetric(metric, row.after),
              change: changeLabel(row.before, row.after),
            }))}
          />
        }
      >
        <DumbbellChart
          rows={rows}
          format={(v) => formatMetric(metric, v)}
          beforeLabel="without AgentTx"
          afterLabel="with AgentTx"
          annotate={(row) => changeLabel(row.before, row.after)}
          ariaLabel={`${METRICS[metric].label} without and with AgentTx by workflow size`}
        />
      </ChartCard>

      <div className="grid gap-4 lg:grid-cols-2">
        <RunPanel title="Without AgentTx" run={selected.baseline} steps={selected.steps} />
        <RunPanel title="With AgentTx" run={selected.agenttx} steps={selected.steps} />
      </div>
    </div>
  );
}
