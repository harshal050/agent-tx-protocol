"use client";

import { useState } from "react";

import { cn } from "../lib/cn";
import { columnPath, linearScale, niceMax } from "../lib/scales";
import { ChartTooltip, TooltipRow, chartColors, useMeasure, type TooltipState } from "./primitives";

export interface ColumnDatum {
  id: string;
  label: string;
  value: number;
}

export interface ColumnChartProps {
  data: ColumnDatum[];
  format: (value: number) => string;
  /** Series name shown in the tooltip. */
  seriesLabel: string;
  height?: number;
  ariaLabel: string;
  className?: string;
}

const MARGIN = { top: 22, right: 12, bottom: 30, left: 60 };
const MAX_COLUMN = 24;

/** Single-series column chart; value labels sit on each cap. */
export function ColumnChart({ data, format, seriesLabel, height = 260, ariaLabel, className }: ColumnChartProps) {
  const [ref, width] = useMeasure<HTMLDivElement>();
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);

  const left = MARGIN.left;
  const right = Math.max(left + 40, width - MARGIN.right);
  const bottom = height - MARGIN.bottom;
  const y = linearScale([0, niceMax(Math.max(0, ...data.map((d) => d.value)), 4)], [bottom, MARGIN.top]);
  const band = (right - left) / Math.max(data.length, 1);
  const columnWidth = Math.min(MAX_COLUMN, band * 0.5);

  return (
    <div ref={ref} className={cn("relative w-full", className)} style={{ height }}>
      {width > 0 && (
        <svg width={width} height={height} role="img" aria-label={ariaLabel}>
          {y.ticks(4).map((tick) => (
            <g key={tick}>
              <line x1={left} x2={right} y1={y(tick)} y2={y(tick)} stroke={chartColors.grid} strokeWidth={1} shapeRendering="crispEdges" />
              <text x={left - 10} y={y(tick)} dy="0.35em" textAnchor="end" className="fill-fg-subtle text-[11px] tabular-nums">
                {format(tick)}
              </text>
            </g>
          ))}
          <line x1={left} x2={right} y1={bottom} y2={bottom} stroke={chartColors.axis} strokeWidth={1} shapeRendering="crispEdges" />
          {data.map((datum, index) => {
            const cx = left + band * index + band / 2;
            const top = y(datum.value);
            const show = () =>
              setTooltip({
                x: cx,
                y: top,
                content: (
                  <>
                    <div className="mb-1 font-medium text-fg">{datum.label}</div>
                    <TooltipRow color={chartColors.after} shape="rect" value={format(datum.value)} label={seriesLabel} />
                  </>
                ),
              });
            return (
              <g
                key={datum.id}
                tabIndex={0}
                role="group"
                aria-label={`${datum.label}: ${format(datum.value)}`}
                onPointerEnter={show}
                onPointerLeave={() => setTooltip(null)}
                onFocus={show}
                onBlur={() => setTooltip(null)}
                className="group outline-none"
              >
                <rect
                  x={cx - band / 2 + 4}
                  y={MARGIN.top - 16}
                  width={Math.max(0, band - 8)}
                  height={bottom - MARGIN.top + 16}
                  rx={8}
                  className="fill-transparent group-hover:fill-surface-2 group-focus-visible:fill-surface-2"
                />
                <path d={columnPath(cx - columnWidth / 2, columnWidth, bottom, top)} fill={chartColors.after} />
                <text x={cx} y={top - 7} textAnchor="middle" className="fill-fg-muted text-[11px] font-medium tabular-nums">
                  {format(datum.value)}
                </text>
                <text x={cx} y={height - 8} textAnchor="middle" className="fill-fg-subtle text-[11px]">
                  {datum.label}
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
