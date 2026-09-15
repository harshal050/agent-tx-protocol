"use client";

import { useState } from "react";

import { cn } from "../lib/cn";
import { linearScale, logDomain, logScale, niceMax, type Scale } from "../lib/scales";
import { ChartTooltip, TooltipRow, chartColors, useMeasure, type TooltipState } from "./primitives";

export interface DumbbellRow {
  id: string;
  label: string;
  before: number;
  after: number;
}

export interface DumbbellChartProps {
  rows: DumbbellRow[];
  format: (value: number) => string;
  beforeLabel: string;
  afterLabel: string;
  scale?: "linear" | "log";
  /** One right-margin annotation per row (e.g. "−42%"). */
  annotate?: (row: DumbbellRow) => string;
  ariaLabel: string;
  className?: string;
}

const ROW_HEIGHT = 44;
const TOP = 6;
const AXIS_HEIGHT = 26;
const DOT_RADIUS = 5;

/** Before → after per row: a de-emphasized "before" dot and an accent "after" dot. */
export function DumbbellChart({
  rows,
  format,
  beforeLabel,
  afterLabel,
  scale = "linear",
  annotate,
  ariaLabel,
  className,
}: DumbbellChartProps) {
  const [ref, width] = useMeasure<HTMLDivElement>();
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const height = TOP + rows.length * ROW_HEIGHT + AXIS_HEIGHT;

  const labelWidth = Math.min(132, Math.max(76, width * 0.22));
  const gutter = annotate ? 70 : 18;
  const plotLeft = labelWidth + 14;
  const plotRight = Math.max(plotLeft + 60, width - gutter);
  const values = rows.flatMap((row) => [row.before, row.after]);

  let x: Scale;
  if (scale === "log") {
    const positive = values.filter((v) => v > 0);
    const domain = positive.length ? logDomain(Math.min(...positive), Math.max(...positive)) : ([1, 10] as [number, number]);
    x = logScale(domain, [plotLeft, plotRight]);
  } else {
    x = linearScale([0, niceMax(Math.max(0, ...values), 4)], [plotLeft, plotRight]);
  }
  const ticks = x.ticks(width < 520 ? 3 : 5);

  const show = (row: DumbbellRow, cy: number) =>
    setTooltip({
      x: Math.max(x(row.before), x(row.after)),
      y: cy,
      content: (
        <>
          <div className="mb-1 font-medium text-fg">{row.label}</div>
          <TooltipRow color={chartColors.before} shape="dot" value={format(row.before)} label={beforeLabel} />
          <TooltipRow color={chartColors.after} shape="dot" value={format(row.after)} label={afterLabel} />
        </>
      ),
    });

  return (
    <div ref={ref} className={cn("relative w-full", className)} style={{ height }}>
      {width > 0 && (
        <svg width={width} height={height} role="img" aria-label={ariaLabel}>
          {ticks.map((tick) => (
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
            const xb = x(row.before);
            const xa = x(row.after);
            return (
              <g
                key={row.id}
                tabIndex={0}
                role="group"
                aria-label={`${row.label}: ${beforeLabel} ${format(row.before)}, ${afterLabel} ${format(row.after)}`}
                onPointerEnter={() => show(row, cy)}
                onPointerLeave={() => setTooltip(null)}
                onFocus={() => show(row, cy)}
                onBlur={() => setTooltip(null)}
                className="group cursor-default outline-none"
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
                <line
                  x1={Math.min(xb, xa)}
                  x2={Math.max(xb, xa)}
                  y1={cy}
                  y2={cy}
                  stroke={chartColors.connector}
                  strokeWidth={2}
                  strokeLinecap="round"
                />
                <circle cx={xb} cy={cy} r={DOT_RADIUS} fill={chartColors.before} stroke={chartColors.surface} strokeWidth={2} />
                <circle cx={xa} cy={cy} r={DOT_RADIUS} fill={chartColors.after} stroke={chartColors.surface} strokeWidth={2} />
                {annotate && (
                  <text
                    x={width - 4}
                    y={cy}
                    dy="0.35em"
                    textAnchor="end"
                    className="fill-fg text-[12px] font-medium tabular-nums"
                  >
                    {annotate(row)}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      )}
      <ChartTooltip state={tooltip} containerWidth={width} />
    </div>
  );
}
