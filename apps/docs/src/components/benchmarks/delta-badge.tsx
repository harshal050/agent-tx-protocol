import { formatPercent, formatRatio, type Comparison } from "@agenttx/benchmarks";
import { Badge } from "@agenttx/ui/primitives";
import { ArrowDown, ArrowUp, Minus } from "lucide-react";

/**
 * Before → after change as a labeled badge. Tone reflects whether the change
 * is an improvement; the label always states it in words.
 */
export function DeltaBadge({ comparison, noun = "" }: { comparison: Comparison; noun?: string }) {
  const suffix = noun ? ` ${noun}` : "";
  if (comparison.winner === "tie") {
    return (
      <Badge tone="neutral">
        <Minus /> no change
      </Badge>
    );
  }
  const improved = comparison.winner === "after";
  const lower = comparison.after < comparison.before;
  const pct = comparison.changePct === null ? null : Math.abs(comparison.changePct);
  const label =
    pct === null
      ? "new"
      : comparison.after === 0
        ? `none${suffix ? ` ${noun}` : ""} left`
        : pct >= 150 && comparison.factor !== null
          ? `${formatRatio(lower ? comparison.factor : 1 / comparison.factor)} ${lower ? "fewer" : "more"}${suffix}`
          : `${formatPercent(pct)} ${lower ? "fewer" : "more"}${suffix}`;

  return (
    <Badge tone={improved ? "success" : "warning"}>
      {lower ? <ArrowDown /> : <ArrowUp />}
      {label}
    </Badge>
  );
}

/** Compact signed percentage for chart annotations, e.g. "−42%". */
export function changeLabel(before: number, after: number): string {
  if (before === after) return "±0%";
  if (before === 0) return "new";
  const pct = ((after - before) / before) * 100;
  return `${pct < 0 ? "−" : "+"}${formatPercent(Math.abs(pct))}`;
}
