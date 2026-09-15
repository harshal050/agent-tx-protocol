import Link from "next/link";

import { contentConfig, repoLinks } from "@/lib/content";

import { GitHubIcon, Logo } from "./brand";

const columns = [
  {
    title: "Learn",
    links: [
      { label: "Introduction", href: "/docs/introduction" },
      { label: "Quickstart", href: "/docs/quickstart" },
      { label: "Rollback strategies", href: "/docs/rollback-strategies" },
      { label: "Benchmarks", href: "/benchmarks" },
    ],
  },
  {
    title: "Reference",
    links: [
      { label: "gRPC API", href: "/docs/grpc-api" },
      { label: "Configuration", href: "/docs/configuration" },
      { label: "Built-in tools", href: "/docs/built-in-tools" },
      { label: "Guarantees & limits", href: "/docs/guarantees" },
    ],
  },
];

export function SiteFooter() {
  const community = [
    { label: "GitHub", href: repoLinks.repo },
    { label: "Issues", href: repoLinks.issues },
    { label: "Contributing", href: "/docs/contributing" },
    { label: "License", href: repoLinks.blob("LICENSE") },
  ];

  return (
    <footer className="border-t border-border">
      <div className="mx-auto grid max-w-7xl gap-10 px-4 py-14 sm:px-6 md:grid-cols-[1.4fr_repeat(3,1fr)]">
        <div className="max-w-xs">
          <Link href="/" className="flex items-center gap-2.5">
            <Logo />
            <span className="font-semibold tracking-tight">AgentTx</span>
          </Link>
          <p className="mt-4 text-sm leading-relaxed text-fg-muted">
            ACID transactions, root-cause rollbacks and Saga compensation for LLM agent tool calls.
          </p>
          <a
            href={repoLinks.repo}
            target="_blank"
            rel="noopener noreferrer"
            className="mt-5 inline-flex items-center gap-2 text-sm text-fg-muted hover:text-fg"
          >
            <GitHubIcon /> {contentConfig.repo}
          </a>
        </div>
        {[...columns, { title: "Community", links: community }].map((column) => (
          <div key={column.title}>
            <h2 className="text-[13px] font-semibold text-fg">{column.title}</h2>
            <ul className="mt-4 space-y-2.5">
              {column.links.map((link) => (
                <li key={link.label}>
                  {link.href.startsWith("/") ? (
                    <Link href={link.href} className="text-sm text-fg-muted transition-colors hover:text-fg">
                      {link.label}
                    </Link>
                  ) : (
                    <a
                      href={link.href}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sm text-fg-muted transition-colors hover:text-fg"
                    >
                      {link.label}
                    </a>
                  )}
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
      <div className="border-t border-border">
        <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-3 px-4 py-6 text-xs text-fg-subtle sm:px-6">
          <span>Apache-2.0 licensed · Built in the open.</span>
          <span>
            Docs and benchmarks load from{" "}
            <a href={repoLinks.repo} className="font-mono hover:text-fg" target="_blank" rel="noopener noreferrer">
              {contentConfig.repo}@{contentConfig.branch}
            </a>
          </span>
        </div>
      </div>
    </footer>
  );
}
