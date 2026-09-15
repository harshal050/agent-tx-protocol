"use client";

import { useState, type KeyboardEvent, type PointerEvent } from "react";

import { cn } from "../lib/cn";
import { linearScale, logDomain, logScale, niceMax, type Scale } from "../lib/scales";
import { ChartTooltip, TooltipRow, chartColors, useMeasure } from "./primitives";

export interface LinePoint {
  x: number;
  y: number;
}

export interface LineSeries {
  id: string;
  label: string;
  color: string;
  points: LinePoint[];
}

export interface LineChartProps {
  series: LineSeries[];
  formatX: (value: number) => string;
  formatY: (value: number) => string;
  xScale?: "linear" | "log";
  yScale?: "linear" | "log";
  /** Horizontal reference line. */
  threshold?: { value: number; label: string };
  height?: number;
  ariaLabel: string;
  className?: string;
}

const MARGIN = { top: 14, right: 104, bottom: 30, left: 60 };

function buildScale(kind: "linear" | "log", min: number, max: number, range: [number, number], exact = false): Scale {
  if (kind === "log") {
    const domain: [number, number] = exact ? [min, max] : logDomain(min, max);
    return logScale(domain, range);
  }
  return linearScale([0, niceMax(max, 4)], range);
}

/** Multi-series line chart with a snapping crosshair and one tooltip listing every series. */
export function LineChart({
  series,
  formatX,
  formatY,
  xScale = "linear",
  yScale = "linear",
  threshold,
  height = 300,
  ariaLabel,
  className,
}: LineChartProps) {
  const [ref, width] = useMeasure<HTMLDivElement>();
  const [active, setActive] = useState<number | null>(null);

  const xs = [...new Set(series.flatMap((s) => s.points.map((p) => p.x)))].sort((a, b) => a - b);
  const ys = series.flatMap((s) => s.points.map((p) => p.y));
  if (threshold) ys.push(threshold.value);

  const left = MARGIN.left;
  const right = Math.max(left + 80, width - MARGIN.right);
  const top = MARGIN.top;
  const bottom = height - MARGIN.bottom;

  const x = buildScale(xScale, xs[0] ?? 1, xs.at(-1) ?? 10, [left, right], true);
  const y = buildScale(yScale, Math.min(...ys), Math.max(...ys), [bottom, top]);
  const xTicks = xScale === "log" ? xs : x.ticks(width < 520 ? 3 : 5);
  const yTicks = y.ticks(4);

  const path = (points: LinePoint[]) =>
    points.map((p, i) => `${i === 0 ? "M" : "L"}${x(p.x).toFixed(2)},${y(p.y).toFixed(2)}`).join(" ");

  const nearestIndex = (px: number) => {
    let best = 0;
    xs.forEach((value, index) => {
      if (Math.abs(x(value) - px) < Math.abs(x(xs[best]!) - px)) best = index;
    });
    return best;
  };

  const onPointerMove = (event: PointerEvent<SVGRectElement>) => {
    const box = event.currentTarget.getBoundingClientRect();
    setActive(nearestIndex(event.clientX - box.left + left));
  };

  const onKeyDown = (event: KeyboardEvent<SVGGElement>) => {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
    event.preventDefault();
    setActive((current) => {
      const start = current ?? (event.key === "ArrowRight" ? -1 : xs.length);
      return Math.min(xs.length - 1, Math.max(0, start + (event.key === "ArrowRight" ? 1 : -1)));
    });
  };

  const activeX = active === null ? null : xs[active];
  const endLabels = series
    .map((s) => ({ s, last: s.points.at(-1) }))
    .filter((entry): entry is { s: LineSeries; last: LinePoint } => entry.last !== undefined)
    .map(({ s, last }) => ({ id: s.id, label: s.label, y: y(last.y) }));
  const labelsCollide = endLabels.some((a, i) => endLabels.some((b, j) => i < j && Math.abs(a.y - b.y) < 16));

  return (
    <div ref={ref} className={cn("relative w-full", className)} style={{ height }}>
      {width > 0 && (
        <svg width={width} height={height} role="img" aria-label={ariaLabel}>
          {yTicks.map((tick) => (
            <g key={`y-${tick}`}>
              <line x1={left} x2={right} y1={y(tick)} y2={y(tick)} stroke={chartColors.grid} strokeWidth={1} shapeRendering="crispEdges" />
              <text x={left - 10} y={y(tick)} dy="0.35em" textAnchor="end" className="fill-fg-subtle text-[11px] tabular-nums">
                {formatY(tick)}
              </text>
            </g>
          ))}
          <line x1={left} x2={right} y1={bottom} y2={bottom} stroke={chartColors.axis} strokeWidth={1} shapeRendering="crispEdges" />
          {xTicks.map((tick) => (
            <text key={`x-${tick}`} x={x(tick)} y={height - 8} textAnchor="middle" className="fill-fg-subtle text-[11px] tabular-nums">
              {formatX(tick)}
            </text>
          ))}

          {threshold && (
            <g>
              <line
                x1={left}
                x2={right}
                y1={y(threshold.value)}
                y2={y(threshold.value)}
                stroke={chartColors.threshold}
                strokeWidth={1}
                shapeRendering="crispEdges"
              />
              <text x={right + 8} y={y(threshold.value)} dy="0.35em" className="fill-fg-subtle text-[11px]">
                {threshold.label}
              </text>
            </g>
          )}

          {series.map((s) => (
            <g key={s.id}>
              <path d={path(s.points)} fill="none" stroke={s.color} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
              {s.points.map((p) => (
                <circle key={p.x} cx={x(p.x)} cy={y(p.y)} r={4} fill={s.color} stroke={chartColors.surface} strokeWidth={2} />
              ))}
            </g>
          ))}

          {!labelsCollide &&
            endLabels.map((label) => (
              <text key={label.id} x={right + 8} y={label.y} dy="0.35em" className="fill-fg-muted text-[11px]">
                {label.label}
              </text>
            ))}

          {activeX !== null && activeX !== undefined && (
            <line x1={x(activeX)} x2={x(activeX)} y1={top} y2={bottom} stroke={chartColors.axis} strokeWidth={1} shapeRendering="crispEdges" />
          )}

          <g tabIndex={0} role="group" aria-label={`${ariaLabel}. Use arrow keys to inspect values.`} onKeyDown={onKeyDown} onBlur={() => setActive(null)} className="outline-none">
            <rect
              x={left}
              y={top}
              width={Math.max(0, right - left)}
              height={Math.max(0, bottom - top)}
              fill="transparent"
              onPointerMove={onPointerMove}
              onPointerLeave={() => setActive(null)}
            />
          </g>
        </svg>
      )}
      <ChartTooltip
        containerWidth={width}
        state={
          activeX === null || activeX === undefined
            ? null
            : {
                x: x(activeX),
                y: top + (bottom - top) / 2,
                content: (
                  <>
                    <div className="mb-1 font-medium text-fg">{formatX(activeX)}</div>
                    {series.map((s) => {
                      const point = s.points.find((p) => p.x === activeX);
                      return point ? <TooltipRow key={s.id} color={s.color} value={formatY(point.y)} label={s.label} /> : null;
                    })}
                  </>
                ),
              }
        }
      />
    </div>
  );
}
