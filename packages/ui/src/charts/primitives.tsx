"use client";

import { useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";

import { ChartIcon, TableIcon } from "../icons";
import { cn } from "../lib/cn";

/**
 * Chart color roles. Values come from CSS custom properties defined by the
 * app theme (validated for CVD separation and contrast in light and dark).
 */
export const chartColors = {
  after: "var(--chart-after)",
  before: "var(--chart-before)",
  series2: "var(--chart-series-2)",
  connector: "var(--chart-connector)",
  grid: "var(--chart-grid)",
  axis: "var(--chart-axis)",
  surface: "var(--chart-surface)",
  threshold: "var(--chart-threshold)",
} as const;

/** Tracks an element's content width. */
export function useMeasure<T extends HTMLElement>(): [RefObject<T | null>, number] {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(Math.round(entry.contentRect.width));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

export type SwatchShape = "dot" | "line" | "rect";

export function Swatch({ color, shape = "dot" }: { color: string; shape?: SwatchShape }) {
  const shapeClass =
    shape === "line" ? "h-0.5 w-3.5 rounded-full" : shape === "rect" ? "h-2.5 w-3 rounded-[2px]" : "size-2.5 rounded-full";
  return <span aria-hidden="true" className={cn("inline-block shrink-0", shapeClass)} style={{ background: color }} />;
}

export interface LegendItem {
  label: string;
  color: string;
  shape?: SwatchShape;
}

export function Legend({ items, className }: { items: LegendItem[]; className?: string }) {
  return (
    <ul className={cn("flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs text-fg-muted", className)}>
      {items.map((item) => (
        <li key={item.label} className="inline-flex items-center gap-1.5">
          <Swatch color={item.color} shape={item.shape} />
          {item.label}
        </li>
      ))}
    </ul>
  );
}

export interface TooltipState {
  x: number;
  y: number;
  content: ReactNode;
}

export function ChartTooltip({ state, containerWidth }: { state: TooltipState | null; containerWidth: number }) {
  if (!state) return null;
  const flip = state.x > containerWidth * 0.58;
  return (
    <div
      role="status"
      className="pointer-events-none absolute z-20 min-w-44 rounded-lg border border-border bg-surface/95 px-3 py-2 text-xs shadow-xl shadow-black/10 backdrop-blur-md dark:shadow-black/40"
      style={{
        left: state.x,
        top: state.y,
        transform: `translate(${flip ? "calc(-100% - 14px)" : "14px"}, -50%)`,
      }}
    >
      {state.content}
    </div>
  );
}

/** Tooltip row: value first (strong), series name second, keyed by a mark swatch. */
export function TooltipRow({
  color,
  shape = "line",
  value,
  label,
}: {
  color: string;
  shape?: SwatchShape;
  value: ReactNode;
  label: ReactNode;
}) {
  return (
    <div className="flex items-center gap-2 py-0.5">
      <Swatch color={color} shape={shape} />
      <span className="font-semibold text-fg tabular-nums">{value}</span>
      <span className="text-fg-muted">{label}</span>
    </div>
  );
}

export function ChartCard({
  title,
  description,
  legend,
  table,
  footer,
  children,
  className,
  id,
}: {
  title: ReactNode;
  description?: ReactNode;
  legend?: ReactNode;
  /** Accessible table twin of the chart. */
  table?: ReactNode;
  footer?: ReactNode;
  children: ReactNode;
  className?: string;
  id?: string;
}) {
  const [view, setView] = useState<"chart" | "table">("chart");
  return (
    <section id={id} className={cn("min-w-0 rounded-2xl border border-border bg-surface p-5 sm:p-6", className)}>
      <header className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0 max-w-2xl">
          <h3 className="text-[15px] font-semibold tracking-tight text-fg">{title}</h3>
          {description && <p className="mt-1 text-sm leading-relaxed text-fg-muted">{description}</p>}
        </div>
        {table && (
          <button
            type="button"
            onClick={() => setView((v) => (v === "chart" ? "table" : "chart"))}
            aria-pressed={view === "table"}
            className="inline-flex h-8 shrink-0 items-center gap-1.5 rounded-md border border-border px-2.5 text-xs font-medium text-fg-muted transition-colors hover:bg-surface-2 hover:text-fg focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/70 [&_svg]:size-3.5"
          >
            {view === "chart" ? <TableIcon /> : <ChartIcon />}
            {view === "chart" ? "Table" : "Chart"}
          </button>
        )}
      </header>
      {legend && view === "chart" && <div className="mb-3">{legend}</div>}
      {view === "chart" ? children : <div className="-mx-3 overflow-x-auto">{table}</div>}
      {footer && <div className="mt-4 border-t border-border pt-3 text-xs text-fg-subtle">{footer}</div>}
    </section>
  );
}
