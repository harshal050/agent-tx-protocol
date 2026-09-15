import { createHmac, timingSafeEqual } from "node:crypto";

import { CONTENT_CACHE_TAG, GITHUB_CACHE_TAG } from "@agenttx/content";
import { revalidateTag } from "next/cache";

/**
 * GitHub push webhook: refreshes docs, benchmark results and repository stats.
 *
 * Configure a webhook on the repository with content type `application/json`,
 * this URL, and the same secret as `GITHUB_WEBHOOK_SECRET`.
 */
export async function POST(request: Request) {
  const secret = process.env.GITHUB_WEBHOOK_SECRET;
  if (!secret) {
    return Response.json({ error: "GITHUB_WEBHOOK_SECRET is not configured" }, { status: 501 });
  }

  const body = await request.text();
  const signature = request.headers.get("x-hub-signature-256") ?? "";
  const expected = `sha256=${createHmac("sha256", secret).update(body).digest("hex")}`;
  const received = Buffer.from(signature);
  const wanted = Buffer.from(expected);
  if (received.length !== wanted.length || !timingSafeEqual(received, wanted)) {
    return Response.json({ error: "invalid signature" }, { status: 401 });
  }

  const event = request.headers.get("x-github-event") ?? "unknown";
  if (event === "ping") {
    return Response.json({ ok: true, event });
  }

  revalidateTag(CONTENT_CACHE_TAG, "max");
  revalidateTag(GITHUB_CACHE_TAG, "max");
  return Response.json({ revalidated: true, event, at: new Date().toISOString() });
}
