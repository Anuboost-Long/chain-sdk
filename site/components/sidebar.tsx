"use client";

import clsx from "clsx";
import Link from "next/link";
import { usePathname } from "next/navigation";

import { nav } from "@/lib/nav";

export function Sidebar({ onNavigate }: Readonly<{ onNavigate?: () => void }>) {
  const pathname = usePathname();

  return (
    <nav aria-label="Documentation">
      {nav.map((section) => (
        <div key={section.title} className="mb-7">
          <h2 className="mb-2 text-ink-faint text-[0.8125rem] font-medium">{section.title}</h2>
          <ul className="border-l border-line">
            {section.pages.map((page) => {
              const active = pathname === page.href;
              return (
                <li key={page.href}>
                  <Link
                    href={page.href}
                    onClick={onNavigate}
                    aria-current={active ? "page" : undefined}
                    className={clsx(
                      "relative -ml-px block",
                      "border-l-2",
                      active ? "border-lime" : "border-transparent",
                      "text-[0.9375rem]",
                      active ? "text-ink font-medium" : "text-ink-muted",
                      "py-1.5 pl-4",
                      "hover:text-ink",
                      !active && "hover:border-ink-faint"
                    )}
                  >
                    {page.title}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
