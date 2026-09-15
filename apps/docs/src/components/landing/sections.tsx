import type { CommitInfo } from "@agenttx/content";
import { buttonVariants } from "@agenttx/ui/button";
import { CopyButton } from "@agenttx/ui/interactive";
import {
  ArrowRight,
  Braces,
  Database,
  Gauge,
  GitBranch,
  History,
  Layers,
  MessageSquareWarning,
  Network,
  ShieldCheck,
  Split,
  Timer,
  Undo2,
  Workflow,
} from "lucide-react";
import Link from "next/link";
import type { ReactNode } from "react";

import { GitHubIcon } from "@/components/brand";
import { SectionHeading } from "@/components/section-heading";

import { RollbackVisual } from "./rollback-visual";

const INSTALL = "cargo run --release -p agenttx-protocol";

/** Short, readable commit headline for the hero badge. */
function commitHeadline(message: string): string {
  const merge = /^Merge pull request #(\d+)/.exec(message);
  if (merge) return `Merged PR #${merge[1]}`;
  return message.length > 56 ? `${message.slice(0, 55).trimEnd()}…` : message;
}

export function Hero({
  repoUrl,
  stars,
  commit,
  restoreLabel,
  traceLabel,
}: {
  repoUrl: string;
  stars: string | null;
  commit: CommitInfo | null;
  restoreLabel?: string;
  traceLabel?: string;
}) {
  return (
    <section className="relative overflow-hidden border-b border-border">
      <div aria-hidden="true" className="bg-grid absolute inset-0" />
      <div aria-hidden="true" className="hero-glow absolute inset-0" />
      <div className="relative mx-auto max-w-7xl px-4 pt-16 pb-20 sm:px-6 sm:pt-24 lg:pt-28 lg:pb-28">
        <div className="mx-auto max-w-3xl text-center">
          <a
            href={commit?.htmlUrl ?? repoUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="animate-fade-up inline-flex max-w-full items-center gap-2 rounded-full sm:max-w-xl border border-border bg-surface/70 py-1 pr-3 pl-1 text-xs text-fg-muted backdrop-blur transition-colors hover:border-border-strong hover:text-fg"
          >
            <span className="shrink-0 rounded-full bg-accent/12 px-2 py-0.5 text-[10.5px] font-semibold tracking-wider whitespace-nowrap text-accent-ink uppercase">
              Open source
            </span>
            <span className="truncate">
              Rust · gRPC · RocksDB
              {commit && (
                <>
                  {" · "}
                  <span className="font-mono">{commit.shortSha}</span> {commitHeadline(commit.message)}
                </>
              )}
            </span>
            <ArrowRight className="size-3.5 shrink-0" />
          </a>

          <h1 className="animate-fade-up mt-7 text-[2.75rem] leading-[1.02] font-semibold tracking-[-0.045em] text-balance text-fg [animation-delay:60ms] sm:text-6xl lg:text-[4.6rem]">
            Transactions for <span className="text-gradient">AI&nbsp;agents</span>.
          </h1>
          <p className="animate-fade-up mx-auto mt-6 max-w-2xl text-[17px] leading-relaxed text-pretty text-fg-muted [animation-delay:120ms] sm:text-xl">
            AgentTx wraps every tool call in an ACID transaction. When a step fails, it finds the step that really
            caused it, rewinds state in milliseconds, undoes side effects and hands your agent a one-line fix.
          </p>

          <div className="animate-fade-up mt-9 flex flex-wrap items-center justify-center gap-3 [animation-delay:180ms]">
            <Link href="/docs/quickstart" className={buttonVariants({ size: "lg" })}>
              Get started <ArrowRight />
            </Link>
            <Link href="/benchmarks" className={buttonVariants({ variant: "secondary", size: "lg" })}>
              <Gauge /> See benchmarks
            </Link>
            <a href={repoUrl} target="_blank" rel="noopener noreferrer" className={buttonVariants({ variant: "ghost", size: "lg" })}>
              <GitHubIcon /> Star{stars ? <span className="tabular-nums">{stars}</span> : null}
            </a>
          </div>

          <div className="animate-fade-up mx-auto mt-8 flex w-fit max-w-full items-center gap-3 rounded-xl border border-border bg-surface/80 py-1.5 pr-1.5 pl-4 font-mono text-[13px] backdrop-blur [animation-delay:220ms]">
            <span className="text-fg-subtle select-none">$</span>
            <code className="truncate text-fg">{INSTALL}</code>
            <CopyButton value={INSTALL} label="Copy install command" />
          </div>
        </div>

        <div className="animate-fade-up mx-auto mt-16 max-w-5xl [animation-delay:300ms]">
          <RollbackVisual restoreLabel={restoreLabel} traceLabel={traceLabel} />
        </div>
      </div>
    </section>
  );
}

const PROBLEMS = [
  {
    icon: Split,
    title: "Silent root causes",
    body: "Step 3 stores the wrong id. Step 9 fails on a foreign key. Retrying step 9 forever never fixes step 3.",
    chip: "retry(step 9) × ∞",
  },
  {
    icon: MessageSquareWarning,
    title: "Poisoned context",
    body: "Every failure pastes kilobytes of stack trace into the prompt, until the model reasons about noise.",
    chip: "at org.postgresql.core.v3…",
  },
  {
    icon: Undo2,
    title: "Half-done side effects",
    body: "Restarting from scratch re-sends emails, leaves draft files behind and trips over rows it already wrote.",
    chip: "email sent ×3",
  },
];

export function Problem() {
  return (
    <section className="mx-auto max-w-7xl px-4 py-24 sm:px-6 sm:py-28">
      <SectionHeading
        eyebrow="The problem"
        title="Agents fail in ways retries can’t fix."
        description="Tool-using agents change real systems. Without transactions, every failure leaves a mess — and a bigger prompt."
      />
      <div className="mt-12 grid gap-4 md:grid-cols-3">
        {PROBLEMS.map(({ icon: Icon, title, body, chip }) => (
          <div key={title} className="group rounded-2xl border border-border bg-surface p-6 transition-colors hover:border-border-strong">
            <div className="flex size-10 items-center justify-center rounded-xl border border-border bg-surface-2 text-fg">
              <Icon className="size-5" />
            </div>
            <h3 className="mt-5 text-lg font-semibold tracking-tight text-fg">{title}</h3>
            <p className="mt-2 text-[15px] leading-relaxed text-fg-muted">{body}</p>
            <div className="mt-5 truncate rounded-lg border border-dashed border-border px-3 py-2 font-mono text-[12px] text-fg-subtle">
              {chip}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

const PIPELINE: { step: string; title: string; body: string; visual: ReactNode }[] = [
  {
    step: "01",
    title: "Clean the error",
    body: "A compiled RegexSet turns any stack trace into one actionable line. No LLM call.",
    visual: (
      <>
        <div className="truncate text-fg-subtle line-through decoration-fg-subtle/40">PSQLException: ERROR: insert or update…</div>
        <div className="mt-1 truncate text-fg">Hint: Foreign key failed for &apos;customer_id&apos;</div>
      </>
    ),
  },
  {
    step: "02",
    title: "Trace the root cause",
    body: "A dependency graph links step outputs to later inputs and walks the key back to its producer.",
    visual: (
      <>
        <div className="text-fg-subtle">step 6 · customer_id</div>
        <div className="mt-1 text-fg">← step 2 · session.value</div>
      </>
    ),
  },
  {
    step: "03",
    title: "Rewind everything",
    body: "Undo-journal snapshots restore state atomically, Saga actions undo files and APIs, staged emails are dropped.",
    visual: (
      <>
        <div className="text-fg-subtle">restore_snapshot(@2)</div>
        <div className="mt-1 text-fg">undo draft.json · drop 1 email</div>
      </>
    ),
  },
  {
    step: "04",
    title: "Resume, bounded",
    body: "The agent resumes at the right step with the hint as a constraint. A replay budget guarantees termination.",
    visual: (
      <>
        <div className="text-fg-subtle">ROLLBACK_TRIGGERED</div>
        <div className="mt-1 text-fg">next_step_id = 2</div>
      </>
    ),
  },
];

export function HowItWorks() {
  return (
    <section className="border-y border-border bg-surface/50">
      <div className="mx-auto max-w-7xl px-4 py-24 sm:px-6 sm:py-28">
        <SectionHeading
          eyebrow="How it works"
          title="One failed step. Four deterministic moves."
          description="Everything below runs inside the proxy in microseconds to milliseconds, before your agent sees a response."
        />
        <ol className="mt-12 grid gap-4 md:grid-cols-2 lg:grid-cols-4">
          {PIPELINE.map((item) => (
            <li key={item.step} className="relative flex flex-col rounded-2xl border border-border bg-surface p-6">
              <span className="font-mono text-[12px] text-accent-ink">{item.step}</span>
              <h3 className="mt-3 text-[17px] font-semibold tracking-tight text-fg">{item.title}</h3>
              <p className="mt-2 flex-1 text-[14.5px] leading-relaxed text-fg-muted">{item.body}</p>
              <div className="mt-5 rounded-lg bg-surface-2 px-3 py-2.5 font-mono text-[11.5px]">{item.visual}</div>
            </li>
          ))}
        </ol>
      </div>
    </section>
  );
}

const FEATURES = [
  { icon: ShieldCheck, title: "ACID overlay", body: "Private per-transaction writes, atomic commit and first-committer-wins conflict detection.", href: "/docs/snapshots" },
  { icon: History, title: "Millisecond rewinds", body: "An undo journal makes snapshots O(1) and restores proportional to changes, not database size.", href: "/docs/snapshots" },
  { icon: Workflow, title: "Dependency graph", body: "Explicit ${steps.N} references and value matching find the step that really broke things.", href: "/docs/dependency-graph" },
  { icon: Undo2, title: "Saga compensation", body: "Crash-recoverable undo actions for files and APIs; emails and webhooks wait until commit.", href: "/docs/saga-compensation" },
  { icon: Braces, title: "Clean Hints", body: "About 30 deterministic rules for SQL, Python, Java, Node, HTTP and more, with a smart fallback.", href: "/docs/clean-hints" },
  { icon: Timer, title: "Bounded by design", body: "Local backtracks, jumps and resets share an O(N log N) replay budget. Loops always end.", href: "/docs/rollback-strategies" },
  { icon: Network, title: "gRPC native", body: "Language-agnostic protobuf contract, health checks and graceful shutdown. Or embed the Rust library.", href: "/docs/grpc-api" },
  { icon: Database, title: "Embedded RocksDB", body: "No external database to run. Statically linked, WAL-durable, with optional fsync per write.", href: "/docs/operations" },
];

export function Features() {
  return (
    <section className="mx-auto max-w-7xl px-4 py-24 sm:px-6 sm:py-28">
      <SectionHeading
        eyebrow="Features"
        title="Production primitives, not a prompt trick."
        description="The guarantees come from the storage engine and the protocol, so they hold no matter which model or framework drives the agent."
      />
      <div className="mt-12 grid gap-px overflow-hidden rounded-2xl border border-border bg-border sm:grid-cols-2 lg:grid-cols-4">
        {FEATURES.map(({ icon: Icon, title, body, href }) => (
          <Link key={title} href={href} className="group flex flex-col bg-surface p-6 transition-colors hover:bg-surface-2/60">
            <Icon className="size-5 text-fg" />
            <h3 className="mt-4 font-semibold tracking-tight text-fg">{title}</h3>
            <p className="mt-2 flex-1 text-[14px] leading-relaxed text-fg-muted">{body}</p>
            <span className="mt-4 inline-flex items-center gap-1 text-[13px] font-medium text-fg-subtle transition-colors group-hover:text-fg">
              Learn more <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" />
            </span>
          </Link>
        ))}
      </div>
    </section>
  );
}

function ArchBox({ title, detail, icon, className }: { title: string; detail: string; icon: ReactNode; className?: string }) {
  return (
    <div className={`rounded-xl border border-border bg-surface p-4 ${className ?? ""}`}>
      <div className="flex items-center gap-2 text-[14px] font-semibold text-fg [&_svg]:size-4">
        {icon}
        {title}
      </div>
      <p className="mt-1.5 text-[13px] leading-relaxed text-fg-muted">{detail}</p>
    </div>
  );
}

export function Architecture() {
  return (
    <section className="border-y border-border bg-surface/50">
      <div className="mx-auto grid max-w-7xl items-center gap-12 px-4 py-24 sm:px-6 sm:py-28 lg:grid-cols-[0.9fr_1.1fr]">
        <SectionHeading
          eyebrow="Architecture"
          title="A single Rust process between your agent and its tools."
          description={
            <>
              Run it as a gRPC proxy next to any framework, or embed the engine as a library. State lives in embedded
              RocksDB; per-transaction mutexes keep steps ordered while transactions run in parallel.{" "}
              <Link href="/docs/architecture" className="font-medium text-fg underline decoration-accent/40 underline-offset-4 hover:decoration-accent">
                Read the architecture guide
              </Link>
              .
            </>
          }
        />
        <div className="relative grid gap-3" role="img" aria-label="Agent sends gRPC calls to AgentTx, which runs tools and stores state in RocksDB.">
          <div className="grid grid-cols-2 gap-3">
            <ArchBox icon={<Layers />} title="Agent runtime" detail="LangGraph, custom loops, MCP hosts — any gRPC client." />
            <ArchBox icon={<GitBranch />} title="Tools" detail="Databases, files, APIs. Emails via the outbox." />
          </div>
          <div className="rounded-2xl border border-accent/30 bg-accent/[0.04] p-4">
            <div className="mb-3 flex items-center justify-between">
              <span className="text-[13px] font-semibold text-fg">AgentTx</span>
              <span className="font-mono text-[11px] text-fg-subtle">agenttx.v1.AgentTxService</span>
            </div>
            <div className="grid grid-cols-2 gap-3">
              <ArchBox icon={<Braces />} title="Error cleaner" detail="RegexSet → Clean Hint" />
              <ArchBox icon={<Timer />} title="Rollback controller" detail="jump · backtrack · reset" />
              <ArchBox icon={<Workflow />} title="Dependency graph" detail="petgraph DAG" />
              <ArchBox icon={<Undo2 />} title="Saga ledger" detail="undo + staging queue" />
            </div>
          </div>
          <ArchBox icon={<Database />} title="RocksDB" detail="overlay state · undo journal · Saga logs — three column families" />
        </div>
      </div>
    </section>
  );
}

export function FinalCta({ repoUrl }: { repoUrl: string }) {
  return (
    <section className="mx-auto max-w-7xl px-4 py-24 sm:px-6 sm:py-28">
      <div className="relative overflow-hidden rounded-3xl border border-border bg-surface px-6 py-16 text-center sm:px-12">
        <div aria-hidden="true" className="hero-glow absolute inset-0 opacity-80" />
        <div className="relative">
          <h2 className="mx-auto max-w-2xl text-3xl leading-tight font-semibold tracking-[-0.03em] text-balance text-fg sm:text-5xl">
            Give your agents a safety net.
          </h2>
          <p className="mx-auto mt-5 max-w-xl text-[17px] text-pretty text-fg-muted">
            Five minutes to your first rollback. Apache-2.0, self-hosted, no telemetry.
          </p>
          <div className="mt-9 flex flex-wrap items-center justify-center gap-3">
            <Link href="/docs/quickstart" className={buttonVariants({ size: "lg" })}>
              Start the quickstart <ArrowRight />
            </Link>
            <a href={repoUrl} target="_blank" rel="noopener noreferrer" className={buttonVariants({ variant: "secondary", size: "lg" })}>
              <GitHubIcon /> View on GitHub
            </a>
          </div>
        </div>
      </div>
    </section>
  );
}
