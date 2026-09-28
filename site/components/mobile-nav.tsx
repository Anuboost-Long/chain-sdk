"use client";

import clsx from "clsx";
import { useRef } from "react";

import { Sidebar } from "@/components/sidebar";
import { useBackdropClose } from "@/lib/use-backdrop-close";

export function MobileNav() {
  const dialog = useRef<HTMLDialogElement>(null);
  const close = () => dialog.current?.close();
  useBackdropClose(dialog);

  return (
    <>
      <button
        type="button"
        onClick={() => dialog.current?.showModal()}
        className={clsx(
          "inline-flex h-9 items-center gap-2 lg:hidden",
          "border border-line rounded-lg",
          "text-ink-muted text-sm",
          "px-3",
          "hover:text-ink"
        )}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true" className="size-4 fill-none stroke-current stroke-[1.5]">
          <path d="M2 4h12M2 8h12M2 12h12" />
        </svg>
        Menu
      </button>
      <dialog
        ref={dialog}
        className={clsx(
          "m-0 h-dvh max-h-none w-[min(20rem,85vw)] max-w-none",
          "bg-canvas backdrop:bg-black/60",
          "border-r border-line",
          "text-ink",
          "p-0"
        )}
      >
        <div className="flex h-full flex-col overflow-y-auto px-5 pt-4 pb-8">
          <button
            type="button"
            onClick={close}
            className={clsx("mb-6 self-end", "text-ink-muted text-sm", "px-2 py-1", "hover:text-ink")}
          >
            Close
          </button>
          <Sidebar onNavigate={close} />
        </div>
      </dialog>
    </>
  );
}
