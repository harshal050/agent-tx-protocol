"use client";

import type { NavSection } from "@agenttx/content";
import { cn } from "@agenttx/ui/lib/cn";
import { ChevronRight } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState } from "react";

function SidebarLinks({ sections, onNavigate }: { sections: NavSection[]; onNavigate?: () => void }) {
  const pathname = usePathname();
  return (
    <nav aria-label="Documentation" className="space-y-7">
      {sections.map((section) => (
        <div key={section.title}>
          <h2 className="mb-2 px-2.5 text-[11.5px] font-semibold tracking-[0.08em] text-fg-subtle uppercase">
            {section.title}
          </h2>
          <ul className="space-y-0.5">
            {section.items.map((item) => {
              const href = `/docs/${item.slug}`;
              const active = pathname === href;
              return (
                <li key={item.slug}>
                  <Link
                    href={href}
                    onClick={onNavigate}
                    aria-current={active ? "page" : undefined}
                    className={cn(
                      "relative block rounded-md px-2.5 py-1.5 text-[13.5px] transition-colors",
                      active
                        ? "bg-surface-2 font-medium text-fg before:absolute before:top-1.5 before:bottom-1.5 before:left-0 before:w-0.5 before:rounded-full before:bg-accent"
                        : "text-fg-muted hover:bg-surface-2/60 hover:text-fg",
                    )}
                  >
                    {item.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}

export function DocsSidebar({ sections }: { sections: NavSection[] }) {
  return <SidebarLinks sections={sections} />;
}

export function DocsMobileNav({ sections }: { sections: NavSection[] }) {
  const pathname = usePathname();
  const [open, setOpen] = useState(false);
  const current = sections.flatMap((s) => s.items).find((item) => `/docs/${item.slug}` === pathname);

  return (
    <div className="sticky top-14 z-30 -mx-4 border-b border-border bg-bg/85 px-4 backdrop-blur-xl sm:-mx-6 sm:px-6 lg:hidden">
      <button
        type="button"
        aria-expanded={open}
        aria-controls="docs-mobile-nav"
        onClick={() => setOpen((v) => !v)}
        className="flex h-11 w-full items-center gap-2 text-[13.5px] text-fg-muted"
      >
        <ChevronRight className={cn("size-4 transition-transform", open && "rotate-90")} />
        <span>Docs</span>
        {current && (
          <>
            <span className="text-fg-subtle">/</span>
            <span className="truncate font-medium text-fg">{current.title}</span>
          </>
        )}
      </button>
      {open && (
        <div id="docs-mobile-nav" className="max-h-[70dvh] overflow-y-auto pt-2 pb-6">
          <SidebarLinks sections={sections} onNavigate={() => setOpen(false)} />
        </div>
      )}
    </div>
  );
}
