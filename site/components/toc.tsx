"use client";

import clsx from "clsx";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";

interface Heading {
  id: string;
  text: string;
  level: 2 | 3;
}

export function Toc() {
  const pathname = usePathname();
  const [headings, setHeadings] = useState<Heading[]>([]);
  const [activeId, setActiveId] = useState<string>();

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.find((entry) => entry.isIntersecting);
        if (visible) setActiveId(visible.target.id);
      },
      { rootMargin: "-80px 0px -70% 0px" }
    );

    // Read the headings once the new page's article has painted.
    const frame = requestAnimationFrame(() => {
      const elements = [...document.querySelectorAll<HTMLHeadingElement>("article h2[id], article h3[id]")];
      setHeadings(
        elements.map((element) => ({
          id: element.id,
          text: element.textContent ?? "",
          level: element.tagName === "H2" ? 2 : 3
        }))
      );
      setActiveId(elements[0]?.id);
      elements.forEach((element) => observer.observe(element));
    });

    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
    };
  }, [pathname]);

  if (headings.length < 2) return null;

  return (
    <nav aria-label="On this page">
      <h2 className="mb-3 text-ink-faint text-[0.8125rem] font-medium">On this page</h2>
      <ul className="space-y-2 text-sm">
        {headings.map((heading) => (
          <li key={heading.id} className={clsx(heading.level === 3 && "pl-3")}>
            <a
              href={`#${heading.id}`}
              className={clsx(heading.id === activeId ? "text-ink" : "text-ink-muted", "hover:text-ink")}
            >
              {heading.text}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}
