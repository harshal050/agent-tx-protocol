"use client";

import { buttonVariants } from "@agenttx/ui/button";
import { RotateCcw } from "lucide-react";
import { useEffect } from "react";

export default function ErrorBoundary({ error, retry }: { error: Error & { digest?: string }; retry: () => void }) {
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <main className="mx-auto flex max-w-xl flex-col items-center px-4 py-28 text-center sm:px-6">
      <p className="font-mono text-sm text-fg-subtle">
        STEP_STATUS_FAILED{error.digest ? ` · ${error.digest}` : ""}
      </p>
      <h1 className="mt-4 text-4xl font-semibold tracking-tight text-fg">Something went wrong.</h1>
      <p className="mt-4 text-fg-muted">
        This page failed to render. Retrying usually helps; if it keeps happening, please open an issue.
      </p>
      <button type="button" onClick={() => retry()} className={buttonVariants({ className: "mt-8" })}>
        <RotateCcw /> Try again
      </button>
    </main>
  );
}
