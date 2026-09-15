"use client";

import { buttonVariants } from "@agenttx/ui/button";
import { cn } from "@agenttx/ui/lib/cn";
import { ArrowUpRight, Menu, X } from "lucide-react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";

interface Item {
  href: string;
  label: string;
}

function isActive(pathname: string, href: string) {
  return pathname === href || pathname.startsWith(`${href}/`);
}

export function HeaderNav({ items }: { items: Item[] }) {
  const pathname = usePathname();
  return (
    <nav aria-label="Main" className="hidden items-center gap-1 md:flex">
      {items.map((item) => {
        const active = isActive(pathname, item.href);
        return (
          <Link
            key={item.href}
            href={item.href}
            aria-current={active ? "page" : undefined}
            className={cn(
              "rounded-md px-2.5 py-1.5 text-[13.5px] font-medium transition-colors",
              active ? "text-fg" : "text-fg-muted hover:text-fg",
            )}
          >
            {item.label}
          </Link>
        );
      })}
    </nav>
  );
}

export function MobileMenu({ items, repoUrl }: { items: Item[]; repoUrl: string }) {
  const [open, setOpen] = useState(false);
  const pathname = usePathname();

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  return (
    <div className="md:hidden">
      <button
        type="button"
        aria-expanded={open}
        aria-controls="mobile-menu"
        aria-label={open ? "Close menu" : "Open menu"}
        onClick={() => setOpen((v) => !v)}
        className={buttonVariants({ variant: "ghost", size: "icon" })}
      >
        {open ? <X /> : <Menu />}
      </button>
      {open && (
        <div id="mobile-menu" className="absolute inset-x-0 top-14 border-b border-border bg-bg/95 px-4 pt-2 pb-5 backdrop-blur-xl">
          <nav aria-label="Mobile" className="flex flex-col">
            {items.map((item) => (
              <Link
                key={item.href}
                href={item.href}
                onClick={() => setOpen(false)}
                aria-current={isActive(pathname, item.href) ? "page" : undefined}
                className="rounded-lg px-3 py-3 text-[15px] font-medium text-fg-muted hover:bg-surface-2 hover:text-fg aria-[current=page]:text-fg"
              >
                {item.label}
              </Link>
            ))}
            <a
              href={repoUrl}
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-1.5 rounded-lg px-3 py-3 text-[15px] font-medium text-fg-muted hover:bg-surface-2 hover:text-fg"
            >
              GitHub <ArrowUpRight className="size-4" />
            </a>
          </nav>
        </div>
      )}
    </div>
  );
}
