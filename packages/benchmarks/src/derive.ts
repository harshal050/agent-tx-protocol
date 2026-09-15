import type { BenchmarkReport, RunMetrics, Scenario, ScenarioKind } from "./schema";

/** A before/after comparison of one metric. */
export interface Comparison {
  before: number;
  after: number;
  /** after − before */
  delta: number;
  /** before ÷ after (how many times smaller "after" is); null when after is 0. */
  factor: number | null;
  /** Relative change from before to after in percent (negative = reduction); null when before is 0. */
  changePct: number | null;
  /** Which side is better given the metric's direction. */
  winner: "before" | "after" | "tie";
}

/** Compares two values; by default lower is better. */
export function compare(before: number, after: number, lowerIsBetter = true): Comparison {
  const delta = after - before;
  const winner =
    before === after ? "tie" : (after < before) === lowerIsBetter ? "after" : "before";
  return {
    before,
    after,
    delta,
    factor: after === 0 ? null : before / after,
    changePct: before === 0 ? null : (delta / before) * 100,
    winner,
  };
}

/** Metrics shown in before/after comparisons, with their direction. */
export const METRICS = {
  tool_executions: { label: "Tool calls", unit: "count", lowerIsBetter: true },
  context_feedback_tokens_est: { label: "Feedback tokens", unit: "tokens", lowerIsBetter: true },
  modeled_latency_ms: { label: "Modeled latency", unit: "ms", lowerIsBetter: true },
  side_effect_damage: { label: "Side-effect damage", unit: "count", lowerIsBetter: true },
} as const;

export type MetricId = keyof typeof METRICS;

/** Duplicate + leaked side effects + orphaned files: everything a run leaves wrong. */
export function sideEffectDamage(run: RunMetrics): number {
  return run.duplicate_side_effects + run.leaked_side_effects + run.orphaned_files;
}

export function metricValue(run: RunMetrics, metric: MetricId): number {
  return metric === "side_effect_damage" ? sideEffectDamage(run) : run[metric];
}

export function compareScenario(scenario: Scenario, metric: MetricId): Comparison {
  return compare(
    metricValue(scenario.baseline, metric),
    metricValue(scenario.agenttx, metric),
    METRICS[metric].lowerIsBetter,
  );
}

export interface ScenarioGroup {
  kind: ScenarioKind;
  title: string;
  description: string;
  scenarios: Scenario[];
}

/** Groups scenarios by failure kind, preserving report order, sizes ascending. */
export function groupScenarios(report: BenchmarkReport): ScenarioGroup[] {
  const groups = new Map<ScenarioKind, ScenarioGroup>();
  for (const scenario of report.simulation) {
    let group = groups.get(scenario.kind);
    if (!group) {
      group = {
        kind: scenario.kind,
        title: scenario.title,
        description: scenario.description,
        scenarios: [],
      };
      groups.set(scenario.kind, group);
    }
    group.scenarios.push(scenario);
  }
  for (const group of groups.values()) group.scenarios.sort((a, b) => a.steps - b.steps);
  return [...groups.values()];
}

/** Picks the representative scenario of a kind: the largest size ≤ `preferredSteps`. */
export function representativeScenario(
  report: BenchmarkReport,
  kind: ScenarioKind,
  preferredSteps = 32,
): Scenario | undefined {
  const candidates = report.simulation
    .filter((s) => s.kind === kind)
    .sort((a, b) => a.steps - b.steps);
  const withinPreferred = candidates.filter((s) => s.steps <= preferredSteps);
  return withinPreferred.at(-1) ?? candidates[0];
}

export interface Headline {
  scenario: Scenario | undefined;
  toolCalls: Comparison | null;
  feedbackTokens: Comparison | null;
  modeledLatency: Comparison | null;
  /** Total side-effect damage across all scenarios. */
  sideEffectDamage: Comparison;
  /** p50 of an engine step in µs. */
  engineP50Us: number | null;
  /** p50 of a gRPC step in µs. */
  grpcP50Us: number | null;
  /** p50 of restoring the largest measured journal ≤ 10k entries, in µs. */
  restore: { entries: number; p50Us: number } | null;
  /** Median context reduction of the error cleaner, in percent. */
  cleanerMedianReductionPct: number | null;
}

function median(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[mid - 1]! + sorted[mid]!) / 2 : sorted[mid]!;
}

/** Headline numbers for the landing page. */
export function headline(report: BenchmarkReport): Headline {
  const scenario = representativeScenario(report, "root-cause");
  const damage = report.simulation.reduce(
    (acc, s) => ({
      before: acc.before + sideEffectDamage(s.baseline),
      after: acc.after + sideEffectDamage(s.agenttx),
    }),
    { before: 0, after: 0 },
  );
  const restorePoint = [...report.snapshot_restore]
    .filter((p) => p.journal_entries <= 10_000)
    .sort((a, b) => a.journal_entries - b.journal_entries)
    .at(-1);

  return {
    scenario,
    toolCalls: scenario ? compareScenario(scenario, "tool_executions") : null,
    feedbackTokens: scenario ? compareScenario(scenario, "context_feedback_tokens_est") : null,
    modeledLatency: scenario ? compareScenario(scenario, "modeled_latency_ms") : null,
    sideEffectDamage: compare(damage.before, damage.after),
    engineP50Us: report.step_latency.find((l) => l.mode === "engine")?.summary.p50_us ?? null,
    grpcP50Us: report.step_latency.find((l) => l.mode === "grpc")?.summary.p50_us ?? null,
    restore: restorePoint
      ? { entries: restorePoint.journal_entries, p50Us: restorePoint.restore.p50_us }
      : null,
    cleanerMedianReductionPct: median(report.error_cleaner.map((c) => c.reduction_pct)),
  };
}
