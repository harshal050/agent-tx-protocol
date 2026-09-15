import { z } from "zod";

export const navItemSchema = z.object({
  slug: z.string().regex(/^[a-z0-9]+(?:-[a-z0-9]+)*$/, "slugs are lowercase kebab-case"),
  title: z.string().min(1),
  file: z.string().regex(/^[\w-]+(?:\/[\w-]+)*\.md$/, "file must be a relative .md path"),
});

export const navSectionSchema = z.object({
  title: z.string().min(1),
  items: z.array(navItemSchema).min(1),
});

export const navigationSchema = z
  .object({ sections: z.array(navSectionSchema).min(1) })
  .superRefine((nav, ctx) => {
    const seen = new Set<string>();
    for (const section of nav.sections) {
      for (const item of section.items) {
        if (seen.has(item.slug)) {
          ctx.addIssue({ code: "custom", message: `duplicate slug "${item.slug}"` });
        }
        seen.add(item.slug);
      }
    }
  });

export type NavItem = z.infer<typeof navItemSchema>;
export type NavSection = z.infer<typeof navSectionSchema>;
export type Navigation = z.infer<typeof navigationSchema>;

export interface FlatNavItem extends NavItem {
  section: string;
  index: number;
}

export function parseNavigation(data: unknown): Navigation {
  return navigationSchema.parse(data);
}

export function flattenNavigation(nav: Navigation): FlatNavItem[] {
  let index = 0;
  return nav.sections.flatMap((section) =>
    section.items.map((item) => ({ ...item, section: section.title, index: index++ })),
  );
}

export interface NavLocation {
  item: FlatNavItem;
  prev: FlatNavItem | null;
  next: FlatNavItem | null;
}

export function locateNavItem(nav: Navigation, slug: string): NavLocation | null {
  const flat = flattenNavigation(nav);
  const position = flat.findIndex((item) => item.slug === slug);
  if (position === -1) return null;
  return {
    item: flat[position]!,
    prev: flat[position - 1] ?? null,
    next: flat[position + 1] ?? null,
  };
}
