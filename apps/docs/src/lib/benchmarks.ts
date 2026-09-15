import "server-only";

import { safeParseReport, type BenchmarkReport } from "@agenttx/benchmarks";
import { ContentSource, type SourceFile } from "@agenttx/content";
import { cache } from "react";

import { contentConfig, contentSource } from "./content";

export const BENCHMARK_RESULTS_PATH = "benchmarks/results/latest.json";

export interface LoadedReport {
  report: BenchmarkReport;
  file: SourceFile;
}

async function tryLoad(source: ContentSource): Promise<LoadedReport | null> {
  const json = await source.readJson(BENCHMARK_RESULTS_PATH);
  if (!json) return null;
  const report = safeParseReport(json.data);
  if (!report) {
    console.warn(`[benchmarks] ${json.file.origin} copy of ${BENCHMARK_RESULTS_PATH} does not match the schema`);
    return null;
  }
  return { report, file: json.file };
}

/**
 * Loads committed benchmark results from GitHub (or the local checkout). If
 * the remote copy predates the current schema, the local file is used instead.
 */
export const getBenchmarkReport = cache(async (): Promise<LoadedReport | null> => {
  try {
    const loaded = await tryLoad(contentSource);
    if (loaded || contentConfig.order.length === 1) return loaded;
    return await tryLoad(new ContentSource({ ...contentConfig, order: ["local"] }));
  } catch (error) {
    console.warn(`[benchmarks] failed to load results: ${error instanceof Error ? error.message : String(error)}`);
    return null;
  }
});
