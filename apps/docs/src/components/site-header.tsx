import { formatCount } from "@agenttx/benchmarks";
import { buttonVariants } from "@agenttx/ui/button";
import Link from "next/link";

import { getRepoStats, repoLinks } from "@/lib/content";
import { siteConfig } from "@/lib/site";

import { GitHubIcon, Logo } from "./brand";
import { CommandMenu } from "./command-menu";
import { HeaderNav, MobileMenu } from "./header-nav";
import { ThemeToggle } from "./theme-toggle";

export async function SiteHeader() {
  const stats = await getRepoStats();

  return (
    <header className="sticky top-0 z-40 border-b border-border bg-bg/70 backdrop-blur-xl">
      <div className="mx-auto flex h-14 max-w-7xl items-center gap-6 px-4 sm:px-6">
        <Link href="/" className="flex shrink-0 items-center gap-2.5 rounded-md" aria-label="AgentTx home">
          <Logo />
          <span className="text-[15px] font-semibold tracking-tight">AgentTx</span>
        </Link>
        <HeaderNav items={[...siteConfig.nav]} />
        <div className="ml-auto flex items-center gap-1">
          <CommandMenu />
          <a
            href={repoLinks.repo}
            target="_blank"
            rel="noopener noreferrer"
            className={buttonVariants({ variant: "ghost", size: "sm", className: "hidden gap-1.5 sm:inline-flex" })}
            aria-label={stats ? `GitHub repository, ${stats.stars} stars` : "GitHub repository"}
          >
            <GitHubIcon />
            <span className="tabular-nums">{stats ? formatCount(stats.stars) : "GitHub"}</span>
          </a>
          <ThemeToggle />
          <MobileMenu items={[...siteConfig.nav]} repoUrl={repoLinks.repo} />
        </div>
      </div>
    </header>
  );
}
