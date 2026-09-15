"use client";

import type { TocEntry } from "@agenttx/content";
import { cn } from "@agenttx/ui/lib/cn";
import { useEffect, useState } from "react";

export function TableOfContents({ entries }: { entries: TocEntry[] }) {
  const [active, setActive] = useState<string | null>(entries[0]?.id ?? null);

  useEffect(() => {
    const headings = entries
      .map((entry) => document.getElementById(entry.id))
      .filter((el): el is HTMLElement => el !== null);
    if (headings.length === 0) return;

    const visible = new Map<string, number>();
    const observer = new IntersectionObserver(
      (records) => {
        for (const record of records) {
          if (record.isIntersecting) visible.set(record.target.id, record.boundingClientRect.top);
          else visible.delete(record.target.id);
        }
        const topmost = [...visible.entries()].sort((a, b) => a[1] - b[1])[0];
        if (topmost) setActive(topmost[0]);
      },
      { rootMargin: "-72px 0px -65% 0px", threshold: [0, 1] },
    );
    headings.forEach((heading) => observer.observe(heading));
    return () => observer.disconnect();
  }, [entries]);

  if (entries.length === 0) return null;

  return (
    <nav aria-label="On this page">
      <h2 className="mb-3 text-[12px] font-semibold tracking-[0.08em] text-fg-subtle uppercase">On this page</h2>
      <ul className="space-y-1 border-l border-border">
        {entries.map((entry) => (
          <li key={entry.id}>
            <a
              href={`#${entry.id}`}
              aria-current={active === entry.id ? "location" : undefined}
              className={cn(
                "-ml-px block border-l py-1 text-[13px] leading-snug transition-colors",
                entry.depth === 3 ? "pl-6" : "pl-3.5",
                active === entry.id
                  ? "border-accent font-medium text-fg"
                  : "border-transparent text-fg-muted hover:border-border-strong hover:text-fg",
              )}
            >
              {entry.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
