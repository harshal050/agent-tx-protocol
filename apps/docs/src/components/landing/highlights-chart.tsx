"use client";

import { formatCount, type Comparison } from "@agenttx/benchmarks";
import { DumbbellChart, type DumbbellRow } from "@agenttx/ui/charts/dumbbell-chart";
import { ChartCard, Legend, chartColors } from "@agenttx/ui/charts/primitives";
import { DataTable } from "@agenttx/ui/primitives";

import { changeLabel } from "@/components/benchmarks/delta-badge";

export function HighlightsChart({ rows, summary }: { rows: DumbbellRow[]; summary: Comparison }) {
  return (
    <ChartCard
      title="Tool calls until the task succeeds — silent root cause"
      description={`A wrong value from an early step fails a later step. Across all sizes: ${formatCount(summary.before)} calls without AgentTx, ${formatCount(summary.after)} with it.`}
      legend={
        <Legend
          items={[
            { label: "Without AgentTx", color: chartColors.before, shape: "dot" },
            { label: "With AgentTx", color: chartColors.after, shape: "dot" },
          ]}
        />
      }
      table={
        <DataTable
          caption="Tool calls by workflow size"
          columns={[
            { key: "size", label: "Workflow" },
            { key: "before", label: "Without AgentTx", align: "right" },
            { key: "after", label: "With AgentTx", align: "right" },
            { key: "change", label: "Change", align: "right" },
          ]}
          rows={rows.map((row) => ({
            size: row.label,
            before: formatCount(row.before),
            after: formatCount(row.after),
            change: changeLabel(row.before, row.after),
          }))}
        />
      }
    >
      <DumbbellChart
        rows={rows}
        format={formatCount}
        beforeLabel="without AgentTx"
        afterLabel="with AgentTx"
        annotate={(row) => changeLabel(row.before, row.after)}
        ariaLabel="Tool calls without and with AgentTx for 8 to 64 step workflows"
      />
    </ChartCard>
  );
}
