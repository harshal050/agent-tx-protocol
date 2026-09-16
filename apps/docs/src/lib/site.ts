export const siteConfig = {
  name: "AgentTx",
  title: "AgentTx — ACID transactions for AI agents",
  tagline: "Transactions for AI agents.",
  description:
    "Open-source Rust proxy that wraps every LLM tool call in a transaction: root-cause rollbacks, millisecond state rewinds, Saga compensation and one-line Clean Hints.",
  url: (process.env.NEXT_PUBLIC_SITE_URL || "https://agenttx.site").replace(/\/$/, ""),
  keywords: [
    "LLM agents",
    "AI agents",
    "transactions",
    "rollback",
    "Saga pattern",
    "RocksDB",
    "gRPC",
    "Rust",
    "tool calling",
  ],
  nav: [
    { href: "/docs", label: "Docs" },
    { href: "/benchmarks", label: "Benchmarks" },
  ],
} as const;

export type NavLink = (typeof siteConfig.nav)[number];
