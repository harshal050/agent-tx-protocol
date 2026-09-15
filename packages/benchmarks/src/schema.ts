import { z } from "zod";

/**
 * Schema for `benchmarks/results/latest.json`, produced by `crates/agenttx-bench`.
 * Mirrors `crates/agenttx-bench/src/report.rs` — keep the two in sync.
 */

export const summarySchema = z.object({
  samples: z.number().int().nonnegative(),
  mean_us: z.number(),
  min_us: z.number(),
  p50_us: z.number(),
  p90_us: z.number(),
  p95_us: z.number(),
  p99_us: z.number(),
  max_us: z.number(),
});

export const runMetricsSchema = z.object({
  succeeded: z.boolean(),
  outcome: z.string(),
  tool_executions: z.number().int().nonnegative(),
  failed_executions: z.number().int().nonnegative(),
  extra_executions: z.number().int().nonnegative(),
  context_feedback_bytes: z.number().int().nonnegative(),
  context_feedback_tokens_est: z.number().int().nonnegative(),
  effects_dispatched: z.number().int().nonnegative(),
  duplicate_side_effects: z.number().int().nonnegative(),
  leaked_side_effects: z.number().int().nonnegative(),
  orphaned_files: z.number().int().nonnegative(),
  wall_time_ms: z.number().nonnegative(),
  modeled_latency_ms: z.number().nonnegative(),
  actions: z.record(z.string(), z.number()),
  events: z.array(z.string()),
});

export const scenarioKindSchema = z.enum(["root-cause", "transient", "persistent"]);

export const scenarioSchema = z.object({
  id: z.string(),
  kind: scenarioKindSchema,
  title: z.string(),
  description: z.string(),
  steps: z.number().int().positive(),
  root_step: z.number().int().nullable(),
  failing_step: z.number().int().positive(),
  baseline: runMetricsSchema,
  agenttx: runMetricsSchema,
});

export const latencyModeSchema = z.enum(["direct", "engine", "grpc"]);

export const latencySchema = z.object({
  mode: latencyModeSchema,
  label: z.string(),
  description: z.string(),
  summary: summarySchema,
});

export const restorePointSchema = z.object({
  journal_entries: z.number().int().positive(),
  restore: summarySchema,
  forward_writes: summarySchema,
});

export const commitPointSchema = z.object({
  keys: z.number().int().positive(),
  commit: summarySchema,
});

export const cleanerResultSchema = z.object({
  id: z.string(),
  label: z.string(),
  runtime: z.string(),
  raw: z.string(),
  raw_bytes: z.number().int().positive(),
  raw_lines: z.number().int().positive(),
  hint_bytes: z.number().int().positive(),
  reduction_pct: z.number(),
  hint: z.string(),
  rule: z.string().nullable(),
  category: z.string(),
  key: z.string().nullable(),
  parse: summarySchema,
});

export const throughputPointSchema = z.object({
  concurrency: z.number().int().positive(),
  transactions: z.number().int().positive(),
  steps_per_transaction: z.number().int().positive(),
  elapsed_ms: z.number().positive(),
  transactions_per_sec: z.number().positive(),
  steps_per_sec: z.number().positive(),
});

export const environmentSchema = z.object({
  os: z.string(),
  arch: z.string(),
  cpu_model: z.string().nullable(),
  logical_cores: z.number().int().positive(),
  memory_gb: z.number().nullable(),
  rustc: z.string().nullable(),
  build_profile: z.string(),
  git_sha: z.string().nullable(),
  git_dirty: z.boolean().nullable(),
});

export const reportSchema = z.object({
  schema_version: z.literal(1),
  generated_at_ms: z.number().int().positive(),
  harness_version: z.string(),
  environment: environmentSchema,
  config: z.object({ quick: z.boolean(), llm_step_ms: z.number().int().nonnegative() }),
  methodology: z.object({
    agent_model: z.string(),
    baseline_policy: z.string(),
    agenttx_policy: z.string(),
    token_estimate: z.string(),
    modeled_latency: z.string(),
  }),
  simulation: z.array(scenarioSchema),
  step_latency: z.array(latencySchema),
  snapshot_restore: z.array(restorePointSchema),
  commit: z.array(commitPointSchema),
  error_cleaner: z.array(cleanerResultSchema),
  throughput: z.array(throughputPointSchema),
});

export type Summary = z.infer<typeof summarySchema>;
export type RunMetrics = z.infer<typeof runMetricsSchema>;
export type ScenarioKind = z.infer<typeof scenarioKindSchema>;
export type Scenario = z.infer<typeof scenarioSchema>;
export type LatencyMode = z.infer<typeof latencyModeSchema>;
export type LatencyResult = z.infer<typeof latencySchema>;
export type RestorePoint = z.infer<typeof restorePointSchema>;
export type CommitPoint = z.infer<typeof commitPointSchema>;
export type CleanerResult = z.infer<typeof cleanerResultSchema>;
export type ThroughputPoint = z.infer<typeof throughputPointSchema>;
export type Environment = z.infer<typeof environmentSchema>;
export type BenchmarkReport = z.infer<typeof reportSchema>;

/** Parses and validates an unknown value as a benchmark report. Throws on mismatch. */
export function parseReport(input: unknown): BenchmarkReport {
  return reportSchema.parse(input);
}

/** Like {@link parseReport} but returns `null` instead of throwing. */
export function safeParseReport(input: unknown): BenchmarkReport | null {
  const result = reportSchema.safeParse(input);
  return result.success ? result.data : null;
}
