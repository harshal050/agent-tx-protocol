import { DocsMobileNav, DocsSidebar } from "@/components/docs/sidebar";
import { getNavigation } from "@/lib/content";

export default async function DocsLayout({ children }: LayoutProps<"/docs">) {
  const navigation = await getNavigation();

  return (
    <div className="mx-auto max-w-7xl px-4 sm:px-6">
      <DocsMobileNav sections={navigation.sections} />
      <div className="lg:grid lg:grid-cols-[232px_minmax(0,1fr)] lg:gap-12">
        <aside className="hidden lg:block">
          <div className="sticky top-14 h-[calc(100dvh-3.5rem)] overflow-y-auto py-10 pr-3">
            <DocsSidebar sections={navigation.sections} />
          </div>
        </aside>
        <main className="min-w-0">{children}</main>
      </div>
    </div>
  );
}
