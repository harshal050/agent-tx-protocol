import GithubSlugger from "github-slugger";
import matter from "gray-matter";
import type { Root as MdastRoot } from "mdast";
import { toString as mdastToString } from "mdast-util-to-string";
import remarkGfm from "remark-gfm";
import remarkParse from "remark-parse";
import { unified } from "unified";

import type { SearchEntry } from "./search-rank";

const MAX_TEXT = 320;

export interface SearchSource {
  slug: string;
  title: string;
  section: string;
  markdown: string;
}

/**
 * Splits a document into one search entry for its introduction and one per
 * h2/h3 section. Anchors match the ids produced by `rehype-slug` when the
 * page is rendered (leading h1 excluded, same slugger sequence).
 */
export function buildSearchEntries(doc: SearchSource): SearchEntry[] {
  const { content } = matter(doc.markdown);
  const tree = unified().use(remarkParse).use(remarkGfm).parse(content) as MdastRoot;
  const slugger = new GithubSlugger();
  const href = `/docs/${doc.slug}`;

  const entries: SearchEntry[] = [];
  let current: { heading: string | null; anchor: string | null; parts: string[] } = {
    heading: null,
    anchor: null,
    parts: [],
  };

  const flush = () => {
    const text = current.parts.join(" ").replace(/\s+/g, " ").trim();
    if (current.heading === null && text === "") return;
    entries.push({
      id: `${doc.slug}${current.anchor ? `#${current.anchor}` : ""}`,
      href: current.anchor ? `${href}#${current.anchor}` : href,
      title: doc.title,
      section: doc.section,
      heading: current.heading,
      text: text.length > MAX_TEXT ? `${text.slice(0, MAX_TEXT - 1)}…` : text,
    });
  };

  tree.children.forEach((node, index) => {
    if (node.type === "heading") {
      const text = mdastToString(node);
      if (node.depth === 1 && index === 0) return;
      const anchor = slugger.slug(text);
      if (node.depth === 2 || node.depth === 3) {
        flush();
        current = { heading: text, anchor, parts: [] };
        return;
      }
      current.parts.push(text);
      return;
    }
    if (node.type === "code") return;
    current.parts.push(mdastToString(node));
  });
  flush();
  return entries;
}
