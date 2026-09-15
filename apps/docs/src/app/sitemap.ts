import { flattenNavigation } from "@agenttx/content";
import type { MetadataRoute } from "next";

import { getNavigation } from "@/lib/content";
import { siteConfig } from "@/lib/site";

export const revalidate = 3600;

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const docs = flattenNavigation(await getNavigation());
  return [
    { url: siteConfig.url, changeFrequency: "weekly", priority: 1 },
    { url: `${siteConfig.url}/benchmarks`, changeFrequency: "weekly", priority: 0.9 },
    ...docs.map((item) => ({
      url: `${siteConfig.url}/docs/${item.slug}`,
      changeFrequency: "weekly" as const,
      priority: 0.8,
    })),
  ];
}
