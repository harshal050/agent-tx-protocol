import { flattenNavigation } from "@agenttx/content";
import { redirect } from "next/navigation";

import { getNavigation } from "@/lib/content";

export default async function DocsIndexPage() {
  const first = flattenNavigation(await getNavigation())[0];
  redirect(first ? `/docs/${first.slug}` : "/");
}
