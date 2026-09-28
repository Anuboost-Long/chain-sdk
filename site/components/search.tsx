"use client";

import clsx from "clsx";
import { useRouter } from "next/navigation";
import { useEffect, useRef, useState } from "react";

import { allPages } from "@/lib/nav";
import { useBackdropClose } from "@/lib/use-backdrop-close";

export function Search() {
  const dialog = useRef<HTMLDialogElement>(null);
  const router = useRouter();
  const [query, setQuery] = useState("");
  useBackdropClose(dialog);

  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  const results = allPages.filter((page) => {
    const haystack = `${page.title} ${page.description}`.toLowerCase();
    return terms.every((term) => haystack.includes(term));
  });

  const open = () => {
    setQuery("");
    dialog.current?.showModal();
  };

  const go = (href: string) => {
    dialog.current?.close();
    router.push(href);
  };

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setQuery("");
        dialog.current?.showModal();
      }
    };
    globalThis.addEventListener("keydown", onKey);
    return () => globalThis.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      <button
        type="button"
        onClick={open}
        className={clsx(
          "inline-flex h-9 w-9 items-center justify-center gap-2 sm:w-full sm:max-w-64 sm:justify-start",
          "bg-raised",
          "border border-line rounded-lg",
          "text-ink-faint text-sm",
          "sm:px-3",
          "hover:text-ink-muted"
        )}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4 shrink-0 fill-none stroke-current stroke-[1.5]">
          <circle cx="7" cy="7" r="4.5" />
          <path d="m10.5 10.5 3 3" />
        </svg>
        <span className="sr-only sm:not-sr-only sm:flex-1 sm:text-left">Search docs</span>
        <kbd className="hidden font-sans text-xs sm:inline">⌘K</kbd>
      </button>
      <dialog
        ref={dialog}
        className={clsx(
          "mx-auto mt-[12vh] w-[min(36rem,calc(100vw-2rem))]",
          "bg-canvas backdrop:bg-black/60",
          "border border-line rounded-xl",
          "text-ink",
          "p-0"
        )}
      >
        <form
          method="dialog"
          onSubmit={(event) => {
            if (!results[0]) return event.preventDefault();
            go(results[0].href);
          }}
        >
          <label htmlFor="docs-search" className="sr-only">
            Search docs
          </label>
          <input
            id="docs-search"
            type="search"
            autoComplete="off"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search pages, like “files” or “migration”"
            className={clsx(
              "w-full",
              "bg-transparent",
              "border-b border-line",
              "text-base placeholder:text-ink-faint",
              "px-4 py-3.5",
              "focus:outline-none"
            )}
          />
        </form>
        <ul className="max-h-[50vh] overflow-y-auto p-2">
          {results.map((page) => (
            <li key={page.href}>
              <button
                type="button"
                onClick={() => go(page.href)}
                className={clsx("block w-full", "rounded-lg", "text-left", "px-3 py-2.5", "hover:bg-raised focus-visible:bg-raised")}
              >
                <span className="block text-ink text-[0.9375rem] font-medium">{page.title}</span>
                <span className="block text-ink-muted text-sm">{page.description}</span>
              </button>
            </li>
          ))}
          {results.length === 0 && (
            <li className="text-ink-muted text-sm px-3 py-6">
              No page matches “{query}”. Try a capability name, like “storage”, or a command, like “build”.
            </li>
          )}
        </ul>
      </dialog>
    </>
  );
}
