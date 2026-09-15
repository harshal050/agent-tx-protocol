import { getSearchEntries } from "@/lib/content";

export const dynamic = "force-static";
export const revalidate = 3600;

export async function GET() {
  const entries = await getSearchEntries();
  return Response.json(entries, {
    headers: { "Cache-Control": "public, max-age=300, s-maxage=3600, stale-while-revalidate=86400" },
  });
}
