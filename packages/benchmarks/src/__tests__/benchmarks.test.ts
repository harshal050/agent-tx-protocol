import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  compare,
  compareScenario,
  formatBytes,
  formatCount,
  formatMicros,
  formatMillis,
  groupScenarios,
  headline,
  parseReport,
  representativeScenario,
  safeParseReport,
  type BenchmarkReport,
  type RunMetrics,
  type Scenario,
  type Summary,
} from "../index";

const here = path.dirname(fileURLToPath(import.meta.url));
const latestPath = path.resolve(here, "../../../../benchmarks/results/latest.json");

function summary(p50: number): Summary {
  return {
    samples: 10,
    mean_us: p50,
    min_us: p50 / 2,
    p50_us: p50,
    p90_us: p50 * 1.5,
    p95_us: p50 * 2,
    p99_us: p50 * 3,
    max_us: p50 * 4,
  };
}

function run(overrides: Partial<RunMetrics>): RunMetrics {
  return {
    succeeded: true,
    outcome: "completed",
    tool_executions: 10,
    failed_executions: 1,
    extra_executions: 2,
    context_feedback_bytes: 400,
    context_feedback_tokens_est: 100,
    effects_dispatched: 2,
    duplicate_side_effects: 0,
    leaked_side_effects: 0,
    orphaned_files: 0,
    wall_time_ms: 3,
    modeled_latency_ms: 10_003,
    actions: {},
    events: [],
    ...overrides,
  };
}

function scenario(steps: number, before: Partial<RunMetrics>, after: Partial<RunMetrics>): Scenario {
  return {
    id: `root-cause-${steps}`,
    kind: "root-cause",
    title: "Silent root cause",
    description: "…",
    steps,
    root_step: steps / 4,
    failing_step: (steps * 3) / 4,
    baseline: run(before),
    agenttx: run(after),
  };
}

function report(): BenchmarkReport {
  return {
    schema_version: 1,
    generated_at_ms: 1_758_000_000_000,
    harness_version: "0.1.0",
    environment: {
      os: "linux",
      arch: "x86_64",
      cpu_model: "Test CPU",
      logical_cores: 8,
      memory_gb: 16,
      rustc: "rustc 1.98.1",
      build_profile: "release",
      git_sha: "abc",
      git_dirty: false,
    },
    config: { quick: false, llm_step_ms: 1000 },
    methodology: {
      agent_model: "a",
      baseline_policy: "b",
      agenttx_policy: "c",
      token_estimate: "d",
      modeled_latency: "e",
    },
    simulation: [
      scenario(64, { tool_executions: 120, orphaned_files: 2 }, { tool_executions: 90 }),
      scenario(16, { tool_executions: 40, duplicate_side_effects: 3 }, { tool_executions: 25 }),
      scenario(32, { tool_executions: 70, context_feedback_tokens_est: 2_000 }, { tool_executions: 45, context_feedback_tokens_est: 30 }),
    ],
    step_latency: [
      { mode: "direct", label: "Direct", description: "", summary: summary(8) },
      { mode: "engine", label: "Engine", description: "", summary: summary(40) },
      { mode: "grpc", label: "gRPC", description: "", summary: summary(180) },
    ],
    snapshot_restore: [
      { journal_entries: 1_000, restore: summary(900), forward_writes: summary(4_000) },
      { journal_entries: 10_000, restore: summary(9_000), forward_writes: summary(40_000) },
      { journal_entries: 100_000, restore: summary(90_000), forward_writes: summary(400_000) },
    ],
    commit: [{ keys: 100, commit: summary(300) }],
    error_cleaner: [
      { id: "a", label: "A", runtime: "Java", raw: "trace a", raw_bytes: 1000, raw_lines: 20, hint_bytes: 100, reduction_pct: 90, hint: "Hint: a", rule: "r", category: "c", key: null, parse: summary(20) },
      { id: "b", label: "B", runtime: "Go", raw: "trace b", raw_bytes: 500, raw_lines: 5, hint_bytes: 100, reduction_pct: 80, hint: "Hint: b", rule: null, category: "unknown", key: "k", parse: summary(30) },
    ],
    throughput: [
      { concurrency: 1, transactions: 100, steps_per_transaction: 20, elapsed_ms: 1000, transactions_per_sec: 100, steps_per_sec: 2000 },
    ],
  };
}

describe("compare", () => {
  it("computes deltas, factors and the winner", () => {
    expect(compare(100, 25)).toEqual({ before: 100, after: 25, delta: -75, factor: 4, changePct: -75, winner: "after" });
    expect(compare(0, 0).winner).toBe("tie");
    expect(compare(5, 0).factor).toBeNull();
    expect(compare(0, 5).changePct).toBeNull();
    expect(compare(10, 20, false).winner).toBe("after");
  });
});

describe("derived metrics", () => {
  it("groups scenarios and sorts sizes", () => {
    const groups = groupScenarios(report());
    expect(groups).toHaveLength(1);
    expect(groups[0]!.scenarios.map((s) => s.steps)).toEqual([16, 32, 64]);
  });

  it("picks the largest scenario within the preferred size", () => {
    expect(representativeScenario(report(), "root-cause")?.steps).toBe(32);
    expect(representativeScenario(report(), "root-cause", 8)?.steps).toBe(16);
    expect(representativeScenario(report(), "transient")).toBeUndefined();
  });

  it("builds headline numbers", () => {
    const h = headline(report());
    expect(h.scenario?.steps).toBe(32);
    expect(h.toolCalls?.factor).toBeCloseTo(70 / 45);
    expect(h.feedbackTokens?.after).toBe(30);
    expect(h.sideEffectDamage).toMatchObject({ before: 5, after: 0 });
    expect(h.engineP50Us).toBe(40);
    expect(h.restore).toEqual({ entries: 10_000, p50Us: 9_000 });
    expect(h.cleanerMedianReductionPct).toBe(85);
    expect(compareScenario(h.scenario!, "side_effect_damage").winner).toBe("tie");
  });
});

describe("schema", () => {
  it("accepts a valid report and rejects a wrong version", () => {
    expect(parseReport(report()).simulation).toHaveLength(3);
    expect(safeParseReport({ ...report(), schema_version: 2 })).toBeNull();
  });

  it.skipIf(!existsSync(latestPath))("validates the committed benchmark results", () => {
    const parsed = parseReport(JSON.parse(readFileSync(latestPath, "utf8")));
    expect(parsed.simulation.length).toBeGreaterThan(0);
    expect(parsed.step_latency.map((l) => l.mode)).toEqual(["direct", "engine", "grpc"]);
  });
});

describe("format", () => {
  it("formats durations, bytes and counts", () => {
    expect(formatMicros(12.345)).toBe("12.3 µs");
    expect(formatMicros(1_540)).toBe("1.54 ms");
    expect(formatMicros(2_500_000)).toBe("2.5 s");
    expect(formatMillis(0.5)).toBe("500 µs");
    expect(formatMillis(125_000)).toBe("2.08 min");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(3_072)).toBe("3 KB");
    expect(formatCount(1_284)).toBe("1,284");
    expect(formatCount(12_900)).toBe("12.9K");
  });
});
