import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import type { Element, Root } from "hast";
import { visit } from "unist-util-visit";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

import {
  ContentSource,
  DocsRepository,
  buildSearchEntries,
  flattenNavigation,
  locateNavItem,
  normalizeRepoPath,
  parseNavigation,
  rankSearch,
  resolveContentConfig,
  resolveRepoPath,
  type FetchLike,
} from "../index";

const NAV = {
  sections: [
    {
      title: "Start",
      items: [
        { slug: "intro", title: "Intro", file: "intro.md" },
        { slug: "guide", title: "Guide", file: "guide.md" },
      ],
    },
    { title: "Reference", items: [{ slug: "api", title: "API", file: "reference/api.md" }] },
  ],
};

const INTRO = `---
title: Intro
description: Welcome.
---

Read the [guide](./guide.md#setup), the [api](reference/api.md) and the [engine](../crates/agenttx-protocol/src/engine/mod.rs).
External [site](https://example.com).

## Getting started

> [!WARNING]
> Build RocksDB first.

\`\`\`rust
fn main() {}
\`\`\`

### Deep dive

Details about \`KeyError\`.

## Getting started
`;

let root: string;

beforeAll(async () => {
  root = await mkdtemp(path.join(tmpdir(), "agenttx-content-"));
  await mkdir(path.join(root, "docs", "reference"), { recursive: true });
  await writeFile(path.join(root, "docs", "navigation.json"), JSON.stringify(NAV));
  await writeFile(path.join(root, "docs", "intro.md"), INTRO);
  await writeFile(path.join(root, "docs", "guide.md"), "# Guide\n\n## Setup\n\nInstall it.\n");
});

afterAll(() => {
  // temp dirs are cleaned by the OS; nothing to release
});

function localRepository(): DocsRepository {
  const config = resolveContentConfig({ AGENTTX_CONTENT_SOURCE: "local", AGENTTX_LOCAL_ROOT: root });
  return new DocsRepository(new ContentSource(config));
}

function findElements(tree: Root, tagName: string): Element[] {
  const found: Element[] = [];
  visit(tree, "element", (node: Element) => {
    if (node.tagName === tagName) found.push(node);
  });
  return found;
}

describe("config", () => {
  it("defaults to GitHub first in production and local first in development", () => {
    expect(resolveContentConfig({ NODE_ENV: "production" }).order).toEqual(["github", "local"]);
    expect(resolveContentConfig({ NODE_ENV: "development" }).order).toEqual(["local", "github"]);
    expect(resolveContentConfig({ AGENTTX_CONTENT_SOURCE: "github" }).order).toEqual(["github"]);
  });

  it("rejects malformed repositories and modes", () => {
    expect(() => resolveContentConfig({ AGENTTX_GITHUB_REPO: "not a repo" })).toThrow();
    expect(() => resolveContentConfig({ AGENTTX_CONTENT_SOURCE: "s3" })).toThrow();
  });
});

describe("navigation", () => {
  it("flattens and locates neighbours", () => {
    const nav = parseNavigation(NAV);
    expect(flattenNavigation(nav).map((i) => i.slug)).toEqual(["intro", "guide", "api"]);
    const location = locateNavItem(nav, "guide");
    expect(location?.prev?.slug).toBe("intro");
    expect(location?.next?.slug).toBe("api");
    expect(location?.item.section).toBe("Start");
    expect(locateNavItem(nav, "missing")).toBeNull();
  });

  it("rejects duplicate slugs", () => {
    const duplicate = {
      sections: [{ title: "A", items: [NAV.sections[0]!.items[0]!, NAV.sections[0]!.items[0]!] }],
    };
    expect(() => parseNavigation(duplicate)).toThrow(/duplicate slug/);
  });
});

describe("paths", () => {
  it("normalizes and guards repository paths", () => {
    expect(normalizeRepoPath("/docs//intro.md")).toBe("docs/intro.md");
    expect(() => normalizeRepoPath("../etc/passwd")).toThrow();
    expect(resolveRepoPath("docs/intro.md", "../crates/x.rs")).toBe("crates/x.rs");
    expect(resolveRepoPath("docs/intro.md", "../../x")).toBeNull();
  });
});

describe("source", () => {
  it("falls back to local files when GitHub returns 404", async () => {
    const calls: string[] = [];
    const fetch404: FetchLike = async (url) => {
      calls.push(url);
      return new Response("missing", { status: 404 });
    };
    const config = resolveContentConfig({ NODE_ENV: "production", AGENTTX_LOCAL_ROOT: root });
    const file = await new ContentSource(config, fetch404).read("docs/intro.md");
    expect(calls[0]).toBe(
      "https://raw.githubusercontent.com/harshal050/agent-tx-protocol/main/docs/intro.md",
    );
    expect(file?.origin).toBe("local");
  });

  it("falls back when GitHub is unreachable but surfaces errors in github-only mode", async () => {
    const offline: FetchLike = async () => {
      throw new TypeError("fetch failed");
    };
    const auto = resolveContentConfig({ NODE_ENV: "production", AGENTTX_LOCAL_ROOT: root });
    expect((await new ContentSource(auto, offline).read("docs/guide.md"))?.origin).toBe("local");

    const githubOnly = resolveContentConfig({ AGENTTX_CONTENT_SOURCE: "github" });
    await expect(new ContentSource(githubOnly, offline).read("docs/guide.md")).rejects.toThrow();
  });

  it("prefers GitHub content when available", async () => {
    const remote: FetchLike = async () => new Response("remote body", { status: 200 });
    const config = resolveContentConfig({ NODE_ENV: "production", AGENTTX_LOCAL_ROOT: root });
    const file = await new ContentSource(config, remote).read("docs/intro.md");
    expect(file).toMatchObject({ origin: "github", content: "remote body" });
    expect(file?.editUrl).toBe("https://github.com/harshal050/agent-tx-protocol/edit/main/docs/intro.md");
  });
});

describe("markdown", () => {
  it("renders metadata, toc, callouts, links and highlighted code", async () => {
    const page = await localRepository().page("intro");
    expect(page).not.toBeNull();
    const { rendered } = page!;
    expect(rendered.title).toBe("Intro");
    expect(rendered.description).toBe("Welcome.");
    expect(rendered.toc).toEqual([
      { id: "getting-started", text: "Getting started", depth: 2 },
      { id: "deep-dive", text: "Deep dive", depth: 3 },
      { id: "getting-started-1", text: "Getting started", depth: 2 },
    ]);

    const hrefs = findElements(rendered.hast, "a").map((a) => a.properties.href);
    expect(hrefs).toContain("/docs/guide#setup");
    expect(hrefs).toContain("/docs/api");
    expect(hrefs).toContain(
      "https://github.com/harshal050/agent-tx-protocol/blob/main/crates/agenttx-protocol/src/engine/mod.rs",
    );

    const asides = findElements(rendered.hast, "aside");
    expect(asides[0]?.properties.dataCallout).toBe("warning");

    const pre = findElements(rendered.hast, "pre")[0];
    expect(pre?.properties["data-language"] ?? pre?.properties.dataLanguage).toBe("rust");
    expect(String(pre?.properties["data-code"] ?? pre?.properties.dataCode)).toBe("fn main() {}");
  });

  it("uses a leading h1 as the title when frontmatter has none", async () => {
    const page = await localRepository().page("guide");
    expect(page?.rendered.title).toBe("Guide");
    expect(page?.next?.slug).toBe("api");
  });

  it("returns null for pages whose file is missing", async () => {
    expect(await localRepository().page("api")).toBeNull();
  });
});

describe("search", () => {
  it("builds per-section entries whose anchors match rendered ids", () => {
    const entries = buildSearchEntries({ slug: "intro", title: "Intro", section: "Start", markdown: INTRO });
    expect(entries.map((e) => e.href)).toEqual([
      "/docs/intro",
      "/docs/intro#getting-started",
      "/docs/intro#deep-dive",
      "/docs/intro#getting-started-1",
    ]);
    expect(entries[2]?.text).toContain("KeyError");
  });

  it("ranks title and heading matches above body matches", () => {
    const entries = [
      { id: "a", href: "/a", title: "Snapshots", section: "Core", heading: null, text: "journal" },
      { id: "b", href: "/b", title: "Intro", section: "Start", heading: "Why a journal", text: "" },
      { id: "c", href: "/c", title: "Other", section: "Ref", heading: null, text: "uses the journal" },
    ];
    expect(rankSearch(entries, "journal").map((e) => e.id)).toEqual(["b", "a", "c"]);
    expect(rankSearch(entries, "snap journal").map((e) => e.id)).toEqual(["a"]);
    expect(rankSearch(entries, "   ")).toEqual([]);
  });
});
