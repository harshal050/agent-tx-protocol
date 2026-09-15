"use client";

import { useState } from "react";

import { cn } from "../lib/cn";
import { horizontalBarPath, logDomain, logScale } from "../lib/scales";
import { ChartTooltip, TooltipRow, chartColors, useMeasure, type TooltipState } from "./primitives";

export interface RangeBarRow {
  id: string;
  label: string;
  p50: number;
  p95: number;
  p99: number;
}

export interface RangeBarChartProps {
  rows: RangeBarRow[];
  format: (value: number) => string;
  ariaLabel: string;
  className?: string;
}

const ROW_HEIGHT = 56;
const BAR = 16;
const TOP = 4;
const AXIS_HEIGHT = 26;

/**
 * Latency distribution per row on a log axis: the bar ends at p50, a whisker
 * runs to p99 and a tick marks p95.
 */
export function RangeBarChart({ rows, format, ariaLabel, className }: RangeBarChartProps) {
  const [ref, width] = useMeasure<HTMLDivElement>();
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const height = TOP + rows.length * ROW_HEIGHT + AXIS_HEIGHT;

  const labelWidth = Math.min(170, Math.max(92, width * 0.26));
  const plotLeft = labelWidth + 16;
  const plotRight = Math.max(plotLeft + 60, width - 78);
  const min = Math.min(...rows.map((r) => r.p50)) * 0.5;
  const max = Math.max(...rows.map((r) => r.p99));
  const x = logScale(logDomain(min, max), [plotLeft, plotRight]);
  const baseline = plotLeft;

  const show = (row: RangeBarRow, cy: number) =>
    setTooltip({
      x: x(row.p99),
      y: cy,
      content: (
        <>
          <div className="mb-1 font-medium text-fg">{row.label}</div>
          <TooltipRow color={chartColors.after} shape="rect" value={format(row.p50)} label="p50" />
          <TooltipRow color={chartColors.after} shape="line" value={format(row.p95)} label="p95" />
          <TooltipRow color={chartColors.after} shape="line" value={format(row.p99)} label="p99" />
        </>
      ),
    });

  return (
    <div ref={ref} className={cn("relative w-full", className)} style={{ height }}>
      {width > 0 && (
        <svg width={width} height={height} role="img" aria-label={ariaLabel}>
          {x.ticks().map((tick) => (
            <g key={tick}>
              <line
                x1={x(tick)}
                x2={x(tick)}
                y1={TOP}
                y2={height - AXIS_HEIGHT}
                stroke={chartColors.grid}
                strokeWidth={1}
                shapeRendering="crispEdges"
              />
              <text x={x(tick)} y={height - 8} textAnchor="middle" className="fill-fg-subtle text-[11px] tabular-nums">
                {format(tick)}
              </text>
            </g>
          ))}
          {rows.map((row, index) => {
            const cy = TOP + index * ROW_HEIGHT + ROW_HEIGHT / 2;
            const x50 = x(row.p50);
            const x95 = x(row.p95);
            const x99 = x(row.p99);
            return (
              <g
                key={row.id}
                tabIndex={0}
                role="group"
                aria-label={`${row.label}: p50 ${format(row.p50)}, p95 ${format(row.p95)}, p99 ${format(row.p99)}`}
                onPointerEnter={() => show(row, cy)}
                onPointerLeave={() => setTooltip(null)}
                onFocus={() => show(row, cy)}
                onBlur={() => setTooltip(null)}
                className="group outline-none"
              >
                <rect
                  x={0}
                  y={cy - ROW_HEIGHT / 2 + 2}
                  width={width}
                  height={ROW_HEIGHT - 4}
                  rx={8}
                  className="fill-transparent group-hover:fill-surface-2 group-focus-visible:fill-surface-2"
                />
                <text x={labelWidth} y={cy} dy="0.35em" textAnchor="end" className="fill-fg-muted text-[12px]">
                  {row.label}
                </text>
                <line x1={x50} x2={x99} y1={cy} y2={cy} stroke={chartColors.after} strokeOpacity={0.45} strokeWidth={2} />
                <line x1={x95} x2={x95} y1={cy - 5} y2={cy + 5} stroke={chartColors.after} strokeOpacity={0.7} strokeWidth={2} />
                <line x1={x99} x2={x99} y1={cy - 7} y2={cy + 7} stroke={chartColors.after} strokeOpacity={0.45} strokeWidth={2} />
                <path d={horizontalBarPath(baseline, x50, cy - BAR / 2, BAR)} fill={chartColors.after} />
                <text x={width - 4} y={cy} dy="0.35em" textAnchor="end" className="fill-fg text-[12px] font-medium tabular-nums">
                  {format(row.p50)}
                </text>
              </g>
            );
          })}
        </svg>
      )}
      <ChartTooltip state={tooltip} containerWidth={width} />
    </div>
  );
}
