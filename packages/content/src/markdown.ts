import path from "node:path";

import rehypeShiki from "@shikijs/rehype";
import type { Element, Root as HastRoot } from "hast";
import matter from "gray-matter";
import { toString as hastToString } from "hast-util-to-string";
import type { Blockquote, Root as MdastRoot } from "mdast";
import { toString as mdastToString } from "mdast-util-to-string";
import rehypeAutolinkHeadings from "rehype-autolink-headings";
import rehypeSlug from "rehype-slug";
import remarkGfm from "remark-gfm";
import remarkParse from "remark-parse";
import remarkRehype from "remark-rehype";
import { codeToHast, type ShikiTransformer } from "shiki";
import { unified } from "unified";
import { visit } from "unist-util-visit";
import { z } from "zod";

export const SHIKI_THEMES = { light: "github-light", dark: "github-dark-default" } as const;

const frontmatterSchema = z.object({
  title: z.string().optional(),
  description: z.string().optional(),
});

export type Frontmatter = z.infer<typeof frontmatterSchema>;

export interface TocEntry {
  id: string;
  text: string;
  depth: 2 | 3;
}

export interface RenderedMarkdown {
  title: string;
  description: string;
  frontmatter: Frontmatter;
  hast: HastRoot;
  toc: TocEntry[];
  readingMinutes: number;
  wordCount: number;
}

/** How relative links inside a Markdown file are rewritten. */
export interface LinkContext {
  /** Repository-relative path of the file being rendered, e.g. `docs/quickstart.md`. */
  sourcePath: string;
  /** Site href for a repository path that is a documentation page. */
  resolveDocHref: (repoPath: string) => string | undefined;
  /** GitHub URL for any other repository path. */
  blobUrl: (repoPath: string) => string;
  /** Raw file URL (for images). */
  rawUrl: (repoPath: string) => string;
}

export const CALLOUT_KINDS = ["note", "tip", "important", "warning", "caution"] as const;
export type CalloutKind = (typeof CALLOUT_KINDS)[number];

const ALERT_MARKER = /^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*\r?\n?/i;

/** Turns GitHub alert blockquotes (`> [!NOTE]`) into `<aside data-callout="note">`. */
function remarkGithubAlerts() {
  return (tree: MdastRoot) => {
    visit(tree, "blockquote", (node: Blockquote) => {
      const paragraph = node.children[0];
      if (paragraph?.type !== "paragraph") return;
      const first = paragraph.children[0];
      if (first?.type !== "text") return;
      const match = ALERT_MARKER.exec(first.value);
      if (!match) return;

      first.value = first.value.slice(match[0].length);
      if (first.value === "") paragraph.children.shift();
      if (paragraph.children[0]?.type === "break") paragraph.children.shift();
      if (paragraph.children.length === 0) node.children.shift();

      Object.assign(node, {
        data: {
          ...node.data,
          hName: "aside",
          hProperties: { dataCallout: match[1]!.toLowerCase() },
        },
      });
    });
  };
}

function isExternal(href: string): boolean {
  return /^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith("//");
}

/** Resolves `relative` against the directory of `from`; null if it escapes the repository. */
export function resolveRepoPath(from: string, relative: string): string | null {
  const joined = path.posix.normalize(path.posix.join(path.posix.dirname(from), relative));
  if (joined === ".." || joined.startsWith("../")) return null;
  return joined.replace(/^\.\//, "").replace(/\/$/, "");
}

function rehypeRewriteLinks(ctx: LinkContext) {
  return (tree: HastRoot) => {
    visit(tree, "element", (node: Element) => {
      if (node.tagName === "a" && typeof node.properties.href === "string") {
        const href = node.properties.href;
        if (isExternal(href)) {
          if (/^https?:/i.test(href)) {
            node.properties.target = "_blank";
            node.properties.rel = ["noopener", "noreferrer"];
          }
          return;
        }
        if (href.startsWith("#") || href.startsWith("/")) return;

        const hashIndex = href.indexOf("#");
        const pathPart = hashIndex === -1 ? href : href.slice(0, hashIndex);
        const hash = hashIndex === -1 ? "" : href.slice(hashIndex);
        const repoPath = resolveRepoPath(ctx.sourcePath, pathPart);
        if (repoPath === null) return;

        const docHref = repoPath.endsWith(".md") ? ctx.resolveDocHref(repoPath) : undefined;
        if (docHref) {
          node.properties.href = `${docHref}${hash}`;
        } else {
          node.properties.href = `${ctx.blobUrl(repoPath)}${hash}`;
          node.properties.target = "_blank";
          node.properties.rel = ["noopener", "noreferrer"];
        }
      }

      if (node.tagName === "img" && typeof node.properties.src === "string") {
        const src = node.properties.src;
        if (!isExternal(src) && !src.startsWith("/")) {
          const repoPath = resolveRepoPath(ctx.sourcePath, src);
          if (repoPath) node.properties.src = ctx.rawUrl(repoPath);
        }
      }
    });
  };
}

function rehypeCollectToc(toc: TocEntry[]) {
  return (tree: HastRoot) => {
    visit(tree, "element", (node: Element) => {
      if ((node.tagName === "h2" || node.tagName === "h3") && typeof node.properties.id === "string") {
        toc.push({
          id: node.properties.id,
          text: hastToString(node).trim(),
          depth: node.tagName === "h2" ? 2 : 3,
        });
      }
    });
  };
}

/** Adds `data-language` and the raw source (`data-code`) to highlighted `<pre>` blocks. */
const metaTransformer: ShikiTransformer = {
  name: "agenttx:code-meta",
  pre(node) {
    node.properties["data-language"] = this.options.lang;
    node.properties["data-code"] = this.source;
  },
};

function firstParagraphText(tree: MdastRoot): string {
  const paragraph = tree.children.find((node) => node.type === "paragraph");
  return paragraph ? mdastToString(paragraph).replace(/\s+/g, " ").trim() : "";
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1).replace(/\s+\S*$/, "")}…`;
}

/** Renders a Markdown document (with frontmatter) to a highlighted hast tree plus metadata. */
export async function renderMarkdown(source: string, links: LinkContext): Promise<RenderedMarkdown> {
  const { data, content } = matter(source);
  const frontmatter = frontmatterSchema.parse(data);

  const mdast = unified().use(remarkParse).use(remarkGfm).parse(content) as MdastRoot;
  let headingTitle: string | undefined;
  const leading = mdast.children[0];
  if (leading?.type === "heading" && leading.depth === 1) {
    headingTitle = mdastToString(leading);
    mdast.children.shift();
  }

  const text = mdastToString(mdast);
  const wordCount = text.split(/\s+/).filter(Boolean).length;
  const toc: TocEntry[] = [];

  const processor = unified()
    .use(remarkGfm)
    .use(remarkGithubAlerts)
    .use(remarkRehype)
    .use(rehypeSlug)
    .use(() => rehypeCollectToc(toc))
    .use(() => rehypeRewriteLinks(links))
    .use(rehypeAutolinkHeadings, {
      behavior: "wrap",
      test: ["h2", "h3", "h4"],
      properties: { className: ["heading-link"] },
    })
    .use(rehypeShiki, {
      themes: SHIKI_THEMES,
      defaultColor: false,
      lazy: true,
      defaultLanguage: "text",
      fallbackLanguage: "text",
      transformers: [metaTransformer],
    });

  const hast = (await processor.run(mdast)) as HastRoot;

  return {
    title: frontmatter.title ?? headingTitle ?? "",
    description: frontmatter.description ?? truncate(firstParagraphText(mdast), 180),
    frontmatter,
    hast,
    toc,
    readingMinutes: Math.max(1, Math.round(wordCount / 220)),
    wordCount,
  };
}

/** Highlights a standalone code snippet with the site's themes. */
export async function highlightCode(code: string, lang: string): Promise<HastRoot> {
  return codeToHast(code.replace(/\n$/, ""), {
    lang,
    themes: SHIKI_THEMES,
    defaultColor: false,
    transformers: [metaTransformer],
  });
}
