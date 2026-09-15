import { cn } from "@agenttx/ui/lib/cn";
import type { ReactNode } from "react";

export function SectionHeading({
  eyebrow,
  title,
  description,
  align = "left",
  className,
  id,
}: {
  eyebrow?: string;
  title: ReactNode;
  description?: ReactNode;
  align?: "left" | "center";
  className?: string;
  id?: string;
}) {
  return (
    <div className={cn("max-w-2xl", align === "center" && "mx-auto text-center", className)}>
      {eyebrow && (
        <p className="mb-3 text-[12.5px] font-semibold tracking-[0.1em] text-accent-ink uppercase">{eyebrow}</p>
      )}
      <h2 id={id} className="scroll-mt-24 text-3xl leading-tight font-semibold tracking-[-0.03em] text-balance text-fg sm:text-4xl">
        {title}
      </h2>
      {description && <p className="mt-4 text-[17px] leading-relaxed text-pretty text-fg-muted">{description}</p>}
    </div>
  );
}
