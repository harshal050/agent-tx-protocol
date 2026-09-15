import type { ComponentProps, ReactNode } from "react";

import {
  AlertTriangleIcon,
  InfoIcon,
  LightbulbIcon,
  OctagonAlertIcon,
  SparkleIcon,
} from "./icons";
import { cn } from "./lib/cn";

export type BadgeTone = "neutral" | "accent" | "success" | "warning" | "danger";

const badgeTones: Record<BadgeTone, string> = {
  neutral: "border-border bg-surface-2 text-fg-muted",
  accent: "border-accent/30 bg-accent/10 text-accent-ink",
  success: "border-success/30 bg-success/10 text-success-ink",
  warning: "border-warning/35 bg-warning/10 text-warning-ink",
  danger: "border-danger/30 bg-danger/10 text-danger-ink",
};

export function Badge({
  tone = "neutral",
  className,
  ...props
}: ComponentProps<"span"> & { tone?: BadgeTone }) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium [&_svg]:size-3.5",
        badgeTones[tone],
        className,
      )}
      {...props}
    />
  );
}

export function Card({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      className={cn("rounded-2xl border border-border bg-surface shadow-[0_1px_0_0_var(--shadow-edge)]", className)}
      {...props}
    />
  );
}

export function Kbd({ className, ...props }: ComponentProps<"kbd">) {
  return (
    <kbd
      className={cn(
        "inline-flex h-5 min-w-5 items-center justify-center rounded border border-border bg-surface-2 px-1 font-sans text-[11px] font-medium text-fg-muted",
        className,
      )}
      {...props}
    />
  );
}

export type CalloutKind = "note" | "tip" | "important" | "warning" | "caution";

const callouts: Record<CalloutKind, { label: string; icon: ReactNode; className: string }> = {
  note: {
    label: "Note",
    icon: <InfoIcon />,
    className: "border-accent/25 bg-accent/[0.06] [--callout-ink:var(--color-accent-ink)]",
  },
  tip: {
    label: "Tip",
    icon: <LightbulbIcon />,
    className: "border-success/25 bg-success/[0.06] [--callout-ink:var(--color-success-ink)]",
  },
  important: {
    label: "Important",
    icon: <SparkleIcon />,
    className: "border-accent/30 bg-accent/[0.08] [--callout-ink:var(--color-accent-ink)]",
  },
  warning: {
    label: "Warning",
    icon: <AlertTriangleIcon />,
    className: "border-warning/35 bg-warning/[0.07] [--callout-ink:var(--color-warning-ink)]",
  },
  caution: {
    label: "Caution",
    icon: <OctagonAlertIcon />,
    className: "border-danger/30 bg-danger/[0.06] [--callout-ink:var(--color-danger-ink)]",
  },
};

export function Callout({
  kind = "note",
  title,
  children,
  className,
}: {
  kind?: CalloutKind;
  title?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  const style = callouts[kind];
  return (
    <aside role="note" className={cn("my-6 rounded-xl border px-4 py-3.5", style.className, className)}>
      <div className="mb-1 flex items-center gap-2 text-sm font-semibold text-(--callout-ink) [&_svg]:size-4">
        {style.icon}
        <span>{title ?? style.label}</span>
      </div>
      <div className="text-[14.5px] leading-relaxed text-fg-muted [&>*:first-child]:mt-0 [&>*:last-child]:mb-0">
        {children}
      </div>
    </aside>
  );
}

export interface StatTileProps {
  label: string;
  value: ReactNode;
  /** Secondary line, e.g. "vs 70 before". */
  detail?: ReactNode;
  /** Optional delta badge or trend. */
  delta?: ReactNode;
  className?: string;
}

export function StatTile({ label, value, detail, delta, className }: StatTileProps) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5 rounded-xl border border-border bg-surface p-4", className)}>
      <div className="text-[13px] text-fg-muted">{label}</div>
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span className="text-[28px] font-semibold leading-none tracking-tight text-fg">{value}</span>
        {delta}
      </div>
      {detail && <div className="text-xs text-fg-subtle">{detail}</div>}
    </div>
  );
}

export interface DataTableColumn {
  key: string;
  label: ReactNode;
  align?: "left" | "right";
}

export function DataTable({
  columns,
  rows,
  caption,
  className,
}: {
  columns: DataTableColumn[];
  rows: Record<string, ReactNode>[];
  caption?: string;
  className?: string;
}) {
  return (
    <table className={cn("w-full border-collapse text-sm", className)}>
      {caption && <caption className="sr-only">{caption}</caption>}
      <thead>
        <tr className="border-b border-border">
          {columns.map((column) => (
            <th
              key={column.key}
              scope="col"
              className={cn(
                "px-3 py-2 text-xs font-medium whitespace-nowrap text-fg-subtle",
                column.align === "right" ? "text-right" : "text-left",
              )}
            >
              {column.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row, index) => (
          <tr key={index} className="border-b border-border/60 last:border-0">
            {columns.map((column) => (
              <td
                key={column.key}
                className={cn(
                  "px-3 py-2 text-fg-muted",
                  column.align === "right" ? "text-right tabular-nums" : "text-left",
                )}
              >
                {row[column.key]}
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
