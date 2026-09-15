import { cn } from "@agenttx/ui/lib/cn";
import type { SVGProps } from "react";

/** AgentTx mark: three committed steps with a rewind arc. */
export function Logo({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" className={cn("size-6", className)} aria-hidden="true">
      <rect width="32" height="32" rx="9" className="fill-fg" />
      <path d="M9 21h14" className="stroke-bg" strokeWidth="2.2" strokeLinecap="round" />
      <circle cx="9" cy="21" r="2.7" className="fill-bg" />
      <circle cx="16" cy="21" r="2.7" className="fill-bg" />
      <circle cx="23" cy="21" r="2.7" fill="var(--accent)" />
      <path d="M22.6 16.3C21 11.2 12.4 10.6 10 15.4" fill="none" stroke="var(--accent)" strokeWidth="2.2" strokeLinecap="round" />
      <path d="M8.9 12.4l1.1 3.3 3.3-1.1" fill="none" stroke="var(--accent)" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function GitHubIcon(props: SVGProps<SVGSVGElement>) {
  return (
    <svg viewBox="0 0 24 24" width={16} height={16} fill="currentColor" aria-hidden="true" {...props}>
      <path d="M12 .5C5.65.5.5 5.65.5 12a11.5 11.5 0 0 0 7.86 10.92c.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.37-3.87-1.37-.53-1.33-1.28-1.69-1.28-1.69-1.05-.72.08-.7.08-.7 1.15.08 1.76 1.19 1.76 1.19 1.03 1.76 2.7 1.25 3.36.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.68 0-1.25.45-2.28 1.19-3.08-.12-.29-.52-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.63 1.59.23 2.76.11 3.05.74.8 1.19 1.83 1.19 3.08 0 4.41-2.7 5.38-5.26 5.67.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 23.5 12C23.5 5.65 18.35.5 12 .5Z" />
    </svg>
  );
}
