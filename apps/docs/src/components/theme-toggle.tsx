"use client";

import { buttonVariants } from "@agenttx/ui/button";
import { Monitor, Moon, Sun } from "lucide-react";
import { useTheme } from "next-themes";
import { useSyncExternalStore } from "react";

const ORDER = ["system", "light", "dark"] as const;
type ThemeChoice = (typeof ORDER)[number];

const noopSubscribe = () => () => {};

export function ThemeToggle() {
  const mounted = useSyncExternalStore(noopSubscribe, () => true, () => false);
  const { theme, setTheme } = useTheme();
  const current: ThemeChoice = mounted && ORDER.includes(theme as ThemeChoice) ? (theme as ThemeChoice) : "system";
  const next = ORDER[(ORDER.indexOf(current) + 1) % ORDER.length]!;
  const Icon = current === "light" ? Sun : current === "dark" ? Moon : Monitor;

  return (
    <button
      type="button"
      onClick={() => setTheme(next)}
      aria-label={`Theme: ${current}. Switch to ${next}.`}
      title={`Theme: ${current}`}
      className={buttonVariants({ variant: "ghost", size: "icon" })}
    >
      <Icon />
    </button>
  );
}
