import { buttonVariants } from "@agenttx/ui/button";
import { ArrowLeft, BookOpen } from "lucide-react";
import Link from "next/link";

export default function NotFound() {
  return (
    <main className="mx-auto flex max-w-xl flex-col items-center px-4 py-28 text-center sm:px-6">
      <p className="font-mono text-sm text-fg-subtle">404 · ROLLBACK_TRIGGERED</p>
      <h1 className="mt-4 text-4xl font-semibold tracking-tight text-fg">This page doesn’t exist.</h1>
      <p className="mt-4 text-fg-muted">
        It may have moved when the docs were updated on GitHub. Resume from a known step:
      </p>
      <div className="mt-8 flex flex-wrap justify-center gap-3">
        <Link href="/" className={buttonVariants({ variant: "secondary" })}>
          <ArrowLeft /> Home
        </Link>
        <Link href="/docs" className={buttonVariants()}>
          <BookOpen /> Documentation
        </Link>
      </div>
    </main>
  );
}
