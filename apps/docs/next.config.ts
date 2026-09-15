import path from "node:path";

import type { NextConfig } from "next";

/** Monorepo root: docs/ and benchmarks/ are read from here as a fallback. */
const monorepoRoot = path.resolve(process.cwd(), "../..");

const securityHeaders = [
  { key: "X-Content-Type-Options", value: "nosniff" },
  { key: "Referrer-Policy", value: "strict-origin-when-cross-origin" },
  { key: "X-Frame-Options", value: "DENY" },
  { key: "Permissions-Policy", value: "camera=(), microphone=(), geolocation=(), interest-cohort=()" },
  { key: "Strict-Transport-Security", value: "max-age=63072000; includeSubDomains; preload" },
];

const nextConfig: NextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  transpilePackages: ["@agenttx/ui", "@agenttx/content", "@agenttx/benchmarks"],
  serverExternalPackages: ["shiki", "@shikijs/rehype", "gray-matter"],
  outputFileTracingRoot: monorepoRoot,
  outputFileTracingIncludes: {
    "/**": ["../../docs/**/*", "../../benchmarks/results/**/*"],
  },
  images: {
    remotePatterns: [{ protocol: "https", hostname: "avatars.githubusercontent.com" }],
  },
  async headers() {
    return [{ source: "/:path*", headers: securityHeaders }];
  },
};

export default nextConfig;
