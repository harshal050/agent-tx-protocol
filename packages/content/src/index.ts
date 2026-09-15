import { resolveContentConfig, type ContentConfig } from "./config";
import { renderMarkdown, type LinkContext, type RenderedMarkdown } from "./markdown";
import {
  flattenNavigation,
  locateNavItem,
  parseNavigation,
  type FlatNavItem,
  type Navigation,
} from "./navigation";
import { buildSearchEntries } from "./search-index";
import type { SearchEntry } from "./search-rank";
import { ContentSource, type FetchLike, type SourceFile } from "./source";

export * from "./config";
export * from "./github";
export * from "./markdown";
export * from "./navigation";
export * from "./search-index";
export * from "./search-rank";
export * from "./source";

export interface DocPage {
  item: FlatNavItem;
  prev: FlatNavItem | null;
  next: FlatNavItem | null;
  file: SourceFile;
  rendered: RenderedMarkdown;
}

/** High-level access to the documentation stored in the repository. */
export class DocsRepository {
  readonly source: ContentSource;

  constructor(source: ContentSource) {
    this.source = source;
  }

  get config(): ContentConfig {
    return this.source.config;
  }

  docPath(item: Pick<FlatNavItem, "file">): string {
    return `${this.config.docsDir}/${item.file}`;
  }

  async navigation(): Promise<Navigation> {
    const json = await this.source.readJson(`${this.config.docsDir}/navigation.json`);
    if (!json) throw new Error(`${this.config.docsDir}/navigation.json was not found`);
    return parseNavigation(json.data);
  }

  linkContext(nav: Navigation, sourcePath: string): LinkContext {
    const hrefByPath = new Map(
      flattenNavigation(nav).map((item) => [this.docPath(item), `/docs/${item.slug}`]),
    );
    return {
      sourcePath,
      resolveDocHref: (repoPath) => hrefByPath.get(repoPath),
      blobUrl: (repoPath) => this.source.urls.blob(repoPath),
      rawUrl: (repoPath) => this.source.urls.raw(repoPath),
    };
  }

  async page(slug: string, nav?: Navigation): Promise<DocPage | null> {
    const navigation = nav ?? (await this.navigation());
    const location = locateNavItem(navigation, slug);
    if (!location) return null;
    const docPath = this.docPath(location.item);
    const file = await this.source.read(docPath);
    if (!file) return null;
    const rendered = await renderMarkdown(file.content, this.linkContext(navigation, docPath));
    return { ...location, file, rendered };
  }

  async searchEntries(nav?: Navigation): Promise<SearchEntry[]> {
    const navigation = nav ?? (await this.navigation());
    const pages = await Promise.all(
      flattenNavigation(navigation).map(async (item) => {
        const file = await this.source.read(this.docPath(item));
        return file
          ? buildSearchEntries({
              slug: item.slug,
              title: item.title,
              section: item.section,
              markdown: file.content,
            })
          : [];
      }),
    );
    return pages.flat();
  }
}

export function createDocsRepository(config?: ContentConfig, fetchImpl?: FetchLike): DocsRepository {
  return new DocsRepository(new ContentSource(config ?? resolveContentConfig(), fetchImpl));
}
