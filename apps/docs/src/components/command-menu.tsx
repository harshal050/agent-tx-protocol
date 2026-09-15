"use client";

import { rankSearch, type SearchEntry } from "@agenttx/content/search-rank";
import { Kbd } from "@agenttx/ui/primitives";
import { cn } from "@agenttx/ui/lib/cn";
import { CornerDownLeft, FileText, Hash, Search } from "lucide-react";
import { useRouter } from "next/navigation";
import { useCallback, useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { createPortal } from "react-dom";

function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

type LoadState = { status: "idle" | "loading" | "error" } | { status: "ready"; entries: SearchEntry[] };

export function CommandMenu() {
  const router = useRouter();
  const listId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [load, setLoad] = useState<LoadState>({ status: "idle" });
  const loadingRef = useRef(false);

  const ensureLoaded = useCallback(() => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    setLoad({ status: "loading" });
    fetch("/api/search")
      .then((response) => {
        if (!response.ok) throw new Error(`search index returned ${response.status}`);
        return response.json() as Promise<SearchEntry[]>;
      })
      .then((entries) => setLoad({ status: "ready", entries }))
      .catch(() => {
        loadingRef.current = false;
        setLoad({ status: "error" });
      });
  }, []);

  const openMenu = useCallback(() => {
    setOpen(true);
    ensureLoaded();
  }, [ensureLoaded]);

  const close = useCallback(() => {
    setOpen(false);
    setQuery("");
    setActive(0);
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const shortcut = event.key.toLowerCase() === "k" && (event.metaKey || event.ctrlKey);
      if (shortcut || (event.key === "/" && !isTypingTarget(event.target))) {
        event.preventDefault();
        openMenu();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [openMenu]);

  useEffect(() => {
    if (!open) return;
    const previous = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = previous;
    };
  }, [open]);

  const entries = load.status === "ready" ? load.entries : [];
  const results = query.trim()
    ? rankSearch(entries, query, 24)
    : entries.filter((entry) => entry.heading === null).slice(0, 24);

  const go = (entry: SearchEntry | undefined) => {
    if (!entry) return;
    close();
    router.push(entry.href);
  };

  const onInputKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next =
        results.length === 0
          ? 0
          : (active + (event.key === "ArrowDown" ? 1 : -1) + results.length) % results.length;
      setActive(next);
      document.getElementById(`${listId}-${next}`)?.scrollIntoView({ block: "nearest" });
    } else if (event.key === "Enter") {
      event.preventDefault();
      go(results[active]);
    } else if (event.key === "Escape") {
      event.preventDefault();
      close();
    }
  };

  return (
    <>
      <button
        type="button"
        onClick={openMenu}
        aria-label="Search documentation"
        className="group inline-flex h-9 items-center gap-2 rounded-lg border border-border bg-surface/60 px-2.5 text-[13px] text-fg-subtle transition-colors hover:border-border-strong hover:text-fg-muted sm:w-56 lg:w-64"
      >
        <Search className="size-4" />
        <span className="hidden flex-1 text-left sm:inline">Search docs…</span>
        <span className="hidden items-center gap-0.5 sm:flex">
          <Kbd>⌘</Kbd>
          <Kbd>K</Kbd>
        </span>
      </button>

      {open &&
        createPortal(
          <div className="fixed inset-0 z-50 flex items-start justify-center px-4 pt-[12vh]" role="presentation">
            <button
              type="button"
              aria-label="Close search"
              tabIndex={-1}
              onClick={close}
              className="absolute inset-0 cursor-default bg-black/40 backdrop-blur-sm"
            />
            <div
              role="dialog"
              aria-modal="true"
              aria-label="Search documentation"
              className="relative w-full max-w-xl overflow-hidden rounded-2xl border border-border bg-surface shadow-2xl shadow-black/20"
            >
              <div className="flex items-center gap-3 border-b border-border px-4">
                <Search className="size-4 shrink-0 text-fg-subtle" />
                <input
                  autoFocus
                  value={query}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    setActive(0);
                  }}
                  onKeyDown={onInputKeyDown}
                  placeholder="Search rollbacks, snapshots, gRPC…"
                  role="combobox"
                  aria-expanded="true"
                  aria-controls={listId}
                  aria-activedescendant={results.length ? `${listId}-${active}` : undefined}
                  aria-autocomplete="list"
                  className="h-13 w-full bg-transparent text-[15px] text-fg outline-none placeholder:text-fg-subtle"
                />
                <Kbd>Esc</Kbd>
              </div>

              <ul id={listId} role="listbox" aria-label="Results" className="max-h-[min(60vh,420px)] overflow-y-auto p-2">
                {load.status === "loading" && <li className="px-3 py-8 text-center text-sm text-fg-subtle">Loading index…</li>}
                {load.status === "error" && (
                  <li className="px-3 py-8 text-center text-sm text-fg-subtle">
                    Search is unavailable right now.{" "}
                    <button type="button" onClick={ensureLoaded} className="font-medium text-fg underline underline-offset-2">
                      Retry
                    </button>
                  </li>
                )}
                {load.status === "ready" && results.length === 0 && (
                  <li className="px-3 py-8 text-center text-sm text-fg-subtle">No results for “{query}”.</li>
                )}
                {results.map((entry, index) => {
                  const selected = index === active;
                  return (
                    <li
                      key={entry.id}
                      id={`${listId}-${index}`}
                      role="option"
                      aria-selected={selected}
                      onPointerMove={() => setActive(index)}
                      onClick={() => go(entry)}
                      className={cn(
                        "flex cursor-pointer items-start gap-3 rounded-lg px-3 py-2.5",
                        selected ? "bg-surface-2" : "hover:bg-surface-2/60",
                      )}
                    >
                      {entry.heading ? (
                        <Hash className="mt-0.5 size-4 shrink-0 text-fg-subtle" />
                      ) : (
                        <FileText className="mt-0.5 size-4 shrink-0 text-fg-subtle" />
                      )}
                      <div className="min-w-0 flex-1">
                        <div className="truncate text-[14px] font-medium text-fg">
                          {entry.heading ?? entry.title}
                        </div>
                        <div className="truncate text-[12.5px] text-fg-subtle">
                          {entry.heading ? `${entry.section} › ${entry.title}` : entry.section}
                          {entry.text ? ` — ${entry.text}` : ""}
                        </div>
                      </div>
                      {selected && <CornerDownLeft className="mt-0.5 size-4 shrink-0 text-fg-subtle" />}
                    </li>
                  );
                })}
              </ul>

              <div className="flex items-center gap-4 border-t border-border px-4 py-2.5 text-[11.5px] text-fg-subtle">
                <span className="flex items-center gap-1">
                  <Kbd>↑</Kbd>
                  <Kbd>↓</Kbd> navigate
                </span>
                <span className="flex items-center gap-1">
                  <Kbd>↵</Kbd> open
                </span>
              </div>
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}
