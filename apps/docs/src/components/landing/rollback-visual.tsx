"use client";

import { cn } from "@agenttx/ui/lib/cn";
import { CircleCheck, CircleX, RotateCcw } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";

type StepState = "pending" | "running" | "done" | "failed" | "rewound";
type Status = "running" | "rolling-back" | "committed";
type Tone = "fail" | "hint" | "rewind" | "ok" | "muted";

interface LogLine {
  id: number;
  tone: Tone;
  text: string;
}

interface Frame {
  states: StepState[];
  logs: LogLine[];
  arc: boolean;
  status: Status;
  hold: number;
}

const STEPS = [
  { tool: "record.insert", what: "customer" },
  { tool: "kv.put", what: "session" },
  { tool: "fs.write", what: "draft" },
  { tool: "kv.put", what: "line items" },
  { tool: "effect.stage", what: "email" },
  { tool: "record.insert", what: "invoice" },
  { tool: "fs.write", what: "report" },
  { tool: "kv.put", what: "status" },
] as const;

const ROOT = 1;
const FAIL = 5;
const MAX_LOGS = 4;

export interface RollbackVisualProps {
  /** Measured restore p50 to quote in the log (e.g. "0.4 ms"). */
  restoreLabel?: string;
  /** Measured raw trace size to quote in the log (e.g. "2.9 KB"). */
  traceLabel?: string;
}

function buildTimeline({ restoreLabel, traceLabel }: RollbackVisualProps): Frame[] {
  const frames: Frame[] = [];
  const states: StepState[] = STEPS.map(() => "pending");
  let logs: LogLine[] = [];
  let arc = false;
  let status: Status = "running";
  let id = 0;

  const log = (tone: Tone, text: string) => {
    logs = [...logs, { id: id++, tone, text }].slice(-MAX_LOGS);
  };
  const push = (hold: number) => frames.push({ states: [...states], logs, arc, status, hold });

  log("muted", "BeginTransaction · agent=billing");
  push(900);
  for (let i = 0; i <= FAIL; i++) {
    states[i] = "running";
    push(260);
    if (i === FAIL) {
      states[i] = "failed";
      log("fail", `✕ step 6 record.insert · ForeignKeyViolation${traceLabel ? ` (${traceLabel} trace)` : ""}`);
      push(1000);
      log("hint", "Hint: Foreign key constraint failed for 'customer_id'.");
      push(1300);
    } else {
      states[i] = "done";
      push(200);
    }
  }
  arc = true;
  status = "rolling-back";
  log("rewind", "↺ dependency jump: customer_id ← step 2");
  push(1100);
  for (let i = FAIL; i >= ROOT; i--) {
    states[i] = "rewound";
    push(140);
  }
  log("rewind", `↺ snapshot restored${restoreLabel ? ` in ${restoreLabel}` : ""} · draft undone · email discarded`);
  push(1400);
  arc = false;
  status = "running";
  for (let i = ROOT; i < STEPS.length; i++) {
    states[i] = "running";
    push(230);
    states[i] = "done";
    push(150);
  }
  status = "committed";
  log("ok", "✓ CommitTransaction · 8 steps · 1 email sent once");
  push(3200);
  return frames;
}

const STATUS_META: Record<Status, { label: string; className: string; icon: typeof CircleCheck }> = {
  running: { label: "Running", className: "border-accent/30 bg-accent/10 text-accent-ink", icon: CircleCheck },
  "rolling-back": { label: "Rolling back", className: "border-warning/35 bg-warning/10 text-warning-ink", icon: RotateCcw },
  committed: { label: "Committed", className: "border-success/30 bg-success/10 text-success-ink", icon: CircleCheck },
};

const TONE_CLASS: Record<Tone, string> = {
  muted: "text-fg-subtle",
  fail: "text-danger-ink",
  hint: "text-fg",
  rewind: "text-warning-ink",
  ok: "text-success-ink",
};

function nodeX(index: number) {
  return ((index + 0.5) / STEPS.length) * 1000;
}

function StepNode({ index, state }: { index: number; state: StepState }) {
  const Icon = state === "failed" ? CircleX : state === "rewound" ? RotateCcw : state === "done" ? CircleCheck : null;
  return (
    <div className="absolute top-[46px] flex -translate-x-1/2 flex-col items-center" style={{ left: `${nodeX(index) / 10}%` }}>
      <motion.div
        animate={{ scale: state === "running" ? 1.12 : 1 }}
        transition={{ type: "spring", stiffness: 420, damping: 22 }}
        className={cn(
          "relative flex size-9 items-center justify-center rounded-full border text-[13px] font-semibold tabular-nums transition-colors duration-300 sm:size-10",
          state === "pending" && "border-border bg-surface text-fg-subtle",
          state === "running" && "border-accent bg-accent/12 text-accent-ink",
          state === "done" && "border-success/45 bg-success/10 text-success-ink",
          state === "failed" && "border-danger/60 bg-danger/12 text-danger-ink",
          state === "rewound" && "border-warning/55 bg-warning/12 text-warning-ink",
        )}
      >
        {state === "running" && <span className="absolute inset-0 animate-ping rounded-full border border-accent/40" />}
        {Icon ? <Icon className="size-4.5" aria-hidden="true" /> : index + 1}
      </motion.div>
      <div className="mt-2.5 hidden text-center md:block">
        <div className="font-mono text-[10.5px] text-fg-muted">{STEPS[index]!.tool}</div>
        <div className="text-[10.5px] text-fg-subtle">{STEPS[index]!.what}</div>
      </div>
    </div>
  );
}

/** Looping illustration of a dependency-jump rollback. */
export function RollbackVisual(props: RollbackVisualProps) {
  const reduceMotion = useReducedMotion();
  const [timeline] = useState(() => buildTimeline(props));
  const [index, setIndex] = useState(0);

  useEffect(() => {
    if (reduceMotion) return;
    const timer = setTimeout(() => setIndex((i) => (i + 1) % timeline.length), timeline[index]!.hold);
    return () => clearTimeout(timer);
  }, [index, reduceMotion, timeline]);

  const frame = reduceMotion ? timeline[timeline.length - 1]! : timeline[index]!;
  const status = STATUS_META[frame.status];
  const StatusIcon = status.icon;
  const arcPath = `M ${nodeX(FAIL)} 40 C ${nodeX(FAIL)} -8, ${nodeX(ROOT)} -8, ${nodeX(ROOT)} 40`;

  return (
    <figure
      className="relative overflow-hidden rounded-2xl border border-border bg-surface/85 shadow-[0_24px_80px_-24px_rgb(0_0_0/0.25)] backdrop-blur-sm"
      aria-label="Animated example: step 6 fails on a foreign key, AgentTx jumps back to step 2, restores state, undoes the draft file, discards the staged email and commits."
    >
      <div className="flex items-center justify-between gap-3 border-b border-border px-4 py-3 sm:px-5">
        <div className="flex min-w-0 items-center gap-3">
          <div className="hidden gap-1.5 sm:flex" aria-hidden="true">
            <span className="size-2.5 rounded-full bg-fg/10" />
            <span className="size-2.5 rounded-full bg-fg/10" />
            <span className="size-2.5 rounded-full bg-fg/10" />
          </div>
          <span className="truncate font-mono text-[12px] text-fg-muted">tx 7f3a · billing-agent · 8 steps</span>
        </div>
        <span className={cn("inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[11.5px] font-medium", status.className)}>
          <StatusIcon className="size-3.5" aria-hidden="true" />
          {status.label}
        </span>
      </div>

      <div className="relative h-[112px] px-2 sm:h-[150px] sm:px-6" aria-hidden="true">
        <div className="relative h-full">
          <svg viewBox="0 0 1000 150" preserveAspectRatio="none" className="absolute inset-0 h-full w-full overflow-visible">
            <line x1={nodeX(0)} x2={nodeX(STEPS.length - 1)} y1={66} y2={66} stroke="var(--border-strong)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
            <AnimatePresence>
              {frame.arc && (
                <motion.path
                  key="arc"
                  d={arcPath}
                  transform="translate(0 26)"
                  fill="none"
                  stroke="var(--warning)"
                  strokeWidth={2}
                  strokeLinecap="round"
                  vectorEffect="non-scaling-stroke"
                  initial={{ pathLength: 0, opacity: 0 }}
                  animate={{ pathLength: 1, opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={{ duration: 0.7, ease: "easeInOut" }}
                />
              )}
            </AnimatePresence>
          </svg>
          <AnimatePresence>
            {frame.arc && (
              <motion.span
                initial={{ opacity: 0, y: 4 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                className="absolute top-1 -translate-x-1/2 rounded-full border border-warning/40 bg-surface px-2 py-0.5 text-[10.5px] font-medium whitespace-nowrap text-warning-ink"
                style={{ left: `${(nodeX(FAIL) + nodeX(ROOT)) / 20}%` }}
              >
                dependency jump
              </motion.span>
            )}
          </AnimatePresence>
          {frame.states.map((state, i) => (
            <StepNode key={i} index={i} state={state} />
          ))}
        </div>
      </div>

      <div className="border-t border-border bg-surface-2/70 px-4 py-3 font-mono text-[11.5px] leading-6 sm:px-5 sm:text-[12.5px]" aria-hidden="true">
        <div className="h-24 overflow-hidden">
          {frame.logs.map((line) => (
            <div key={line.id} className={cn("animate-fade-up truncate [animation-duration:250ms]", TONE_CLASS[line.tone])}>
              {line.text}
            </div>
          ))}
        </div>
      </div>
    </figure>
  );
}
