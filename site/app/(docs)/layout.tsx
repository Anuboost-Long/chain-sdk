import { PageNav } from "@/components/page-nav";
import { Sidebar } from "@/components/sidebar";
import { Toc } from "@/components/toc";

export default function DocsLayout({ children }: LayoutProps<"/">) {
  return (
    <div className="mx-auto grid max-w-7xl gap-10 px-4 sm:px-6 lg:grid-cols-[14rem_minmax(0,1fr)] xl:grid-cols-[14rem_minmax(0,1fr)_12rem]">
      <aside className="sticky top-14 hidden h-[calc(100dvh-3.5rem)] overflow-y-auto py-10 lg:block">
        <Sidebar />
      </aside>
      <main className="min-w-0 pt-10 pb-24">
        <article className="prose">{children}</article>
        <div className="max-w-[72ch]">
          <PageNav />
        </div>
      </main>
      <aside className="sticky top-14 hidden h-[calc(100dvh-3.5rem)] overflow-y-auto py-10 xl:block">
        <Toc />
      </aside>
    </div>
  );
}
