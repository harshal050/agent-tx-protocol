import { flattenNavigation } from "@agenttx/content";
import { Badge } from "@agenttx/ui/primitives";
import { ArrowLeft, ArrowRight, ChevronRight, Clock, GitCommitHorizontal, History, MessageSquareWarning, Pencil } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";

import { TableOfContents } from "@/components/docs/toc";
import { GitHubIcon } from "@/components/brand";
import { Markdown } from "@/components/markdown";
import { contentConfig, getDocPage, getFileLastCommit, getNavigation, repoLinks } from "@/lib/content";

export const revalidate = 3600;

export async function generateStaticParams() {
  return flattenNavigation(await getNavigation()).map((item) => ({ slug: item.slug }));
}

export async function generateMetadata({ params }: PageProps<"/docs/[slug]">): Promise<Metadata> {
  const { slug } = await params;
  const page = await getDocPage(slug);
  if (!page) return {};
  const title = page.rendered.title || page.item.title;
  return {
    title,
    description: page.rendered.description,
    alternates: { canonical: `/docs/${slug}` },
    openGraph: { type: "article", title, description: page.rendered.description, url: `/docs/${slug}` },
  };
}

const dateFormat = new Intl.DateTimeFormat("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });

export default async function DocPage({ params }: PageProps<"/docs/[slug]">) {
  const { slug } = await params;
  const page = await getDocPage(slug);
  if (!page) notFound();

  const lastCommit = await getFileLastCommit(page.file.path);
  const title = page.rendered.title || page.item.title;
  const issueUrl = `${repoLinks.issues}/new?${new URLSearchParams({
    title: `docs(${slug}): `,
    body: `Page: /docs/${slug}\nSource: ${page.file.htmlUrl}\n\n`,
  }).toString()}`;

  return (
    <div className="xl:grid xl:grid-cols-[minmax(0,1fr)_208px] xl:gap-12">
      <article className="min-w-0 pt-8 pb-16 lg:pt-12">
        <nav aria-label="Breadcrumb" className="mb-5 flex items-center gap-1 text-[13px] text-fg-subtle">
          <Link href="/docs" className="hover:text-fg">
            Docs
          </Link>
          <ChevronRight className="size-3.5" />
          <span>{page.item.section}</span>
        </nav>

        <header className="max-w-3xl">
          <h1 className="text-[2rem] leading-tight font-semibold tracking-[-0.03em] text-balance text-fg sm:text-[2.5rem]">
            {title}
          </h1>
          {page.rendered.description && (
            <p className="mt-4 text-[17px] leading-relaxed text-pretty text-fg-muted">{page.rendered.description}</p>
          )}
          <div className="mt-6 flex flex-wrap items-center gap-2 text-xs text-fg-subtle">
            <Badge>
              <Clock /> {page.rendered.readingMinutes} min read
            </Badge>
            <Badge tone={page.file.origin === "github" ? "accent" : "neutral"}>
              {page.file.origin === "github" ? <GitHubIcon /> : <GitCommitHorizontal />}
              {page.file.origin === "github" ? `Synced from ${contentConfig.branch}` : "Local preview"}
            </Badge>
            {lastCommit?.date && (
              <a href={lastCommit.htmlUrl} target="_blank" rel="noopener noreferrer" className="ml-1 hover:text-fg">
                Updated {dateFormat.format(new Date(lastCommit.date))}
              </a>
            )}
          </div>
        </header>

        <div className="docs-prose mt-10 max-w-3xl">
          <Markdown tree={page.rendered.hast} />
        </div>

        <nav aria-label="Pagination" className="mt-16 grid max-w-3xl gap-3 sm:grid-cols-2">
          {page.prev ? (
            <Link
              href={`/docs/${page.prev.slug}`}
              className="group rounded-xl border border-border p-4 transition-colors hover:border-border-strong hover:bg-surface"
            >
              <span className="flex items-center gap-1.5 text-xs text-fg-subtle">
                <ArrowLeft className="size-3.5 transition-transform group-hover:-translate-x-0.5" /> Previous
              </span>
              <span className="mt-1 block font-medium text-fg">{page.prev.title}</span>
            </Link>
          ) : (
            <span />
          )}
          {page.next && (
            <Link
              href={`/docs/${page.next.slug}`}
              className="group rounded-xl border border-border p-4 text-right transition-colors hover:border-border-strong hover:bg-surface"
            >
              <span className="flex items-center justify-end gap-1.5 text-xs text-fg-subtle">
                Next <ArrowRight className="size-3.5 transition-transform group-hover:translate-x-0.5" />
              </span>
              <span className="mt-1 block font-medium text-fg">{page.next.title}</span>
            </Link>
          )}
        </nav>

        <div className="mt-10 flex max-w-3xl flex-wrap items-center gap-x-5 gap-y-2 border-t border-border pt-6 text-[13px] text-fg-muted xl:hidden">
          <a href={page.file.editUrl} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1.5 hover:text-fg">
            <Pencil className="size-3.5" /> Edit this page
          </a>
          <a href={issueUrl} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1.5 hover:text-fg">
            <MessageSquareWarning className="size-3.5" /> Report an issue
          </a>
        </div>
      </article>

      <aside className="hidden xl:block">
        <div className="sticky top-14 max-h-[calc(100dvh-3.5rem)] overflow-y-auto pt-12 pb-10">
          <TableOfContents entries={page.rendered.toc} />
          <div className="mt-8 space-y-2.5 border-t border-border pt-5 text-[13px] text-fg-muted">
            <a href={page.file.editUrl} target="_blank" rel="noopener noreferrer" className="flex items-center gap-2 hover:text-fg">
              <Pencil className="size-3.5" /> Edit this page
            </a>
            <a href={repoLinks.commits(page.file.path)} target="_blank" rel="noopener noreferrer" className="flex items-center gap-2 hover:text-fg">
              <History className="size-3.5" /> Page history
            </a>
            <a href={issueUrl} target="_blank" rel="noopener noreferrer" className="flex items-center gap-2 hover:text-fg">
              <MessageSquareWarning className="size-3.5" /> Report an issue
            </a>
          </div>
        </div>
      </aside>
    </div>
  );
}
