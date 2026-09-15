import { contentSource } from "@/lib/content";

export const revalidate = 3600;

const CONTENT_TYPES: Record<string, string> = {
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  svg: "image/svg+xml",
};

/** Serves images from the repository's docs/ folder (GitHub first, local checkout as fallback). */
export async function GET(_request: Request, context: RouteContext<"/docs-assets/[...path]">) {
  const { path } = await context.params;
  const repoPath = path.join("/");
  const extension = repoPath.split(".").pop()?.toLowerCase() ?? "";
  const contentType = CONTENT_TYPES[extension];
  if (!contentType || !repoPath.startsWith("docs/") || path.some((segment) => segment === ".." || segment === "")) {
    return new Response("Not found", { status: 404 });
  }

  const file = await contentSource.readBytes(repoPath).catch(() => null);
  if (!file) return new Response("Not found", { status: 404 });

  return new Response(new Uint8Array(file.bytes), {
    headers: {
      "Content-Type": contentType,
      "Cache-Control": "public, max-age=3600, s-maxage=86400, stale-while-revalidate=604800",
      ...(extension === "svg" ? { "Content-Security-Policy": "default-src 'none'; style-src 'unsafe-inline'" } : {}),
    },
  });
}
