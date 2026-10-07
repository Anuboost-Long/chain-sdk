"use client";

import clsx from "clsx";
import Link from "next/link";
import { usePathname } from "next/navigation";

import { allPages, type NavPage } from "@/lib/nav";

function PageLink({ page, direction }: Readonly<{ page: NavPage; direction: "Previous" | "Next" }>) {
  return (
    <Link
      href={page.href}
      className={clsx(
        "flex flex-1 flex-col gap-1",
        "border border-line rounded-xl",
        direction === "Next" && "text-right",
        "px-4 py-3",
        "hover:border-ink-faint"
      )}
    >
      <span className="text-ink-faint text-sm">{direction}</span>
      <span className="text-ink font-medium">{page.title}</span>
    </Link>
  );
}

export function PageNav() {
  const pathname = usePathname();
  const index = allPages.findIndex((page) => page.href === pathname);
  if (index === -1) return null;
  const previous = allPages[index - 1];
  const next = allPages[index + 1];

  return (
    <nav aria-label="Previous and next page" className="mt-16 flex gap-4">
      {previous ? <PageLink page={previous} direction="Previous" /> : <span className="flex-1" />}
      {next ? <PageLink page={next} direction="Next" /> : <span className="flex-1" />}
    </nav>
  );
}
