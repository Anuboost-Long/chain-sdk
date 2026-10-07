import clsx from "clsx";
import type { Metadata } from "next";
import { Bricolage_Grotesque, JetBrains_Mono, Onest } from "next/font/google";
import Link from "next/link";

import { ChainMark } from "@/components/logo";
import { MobileNav } from "@/components/mobile-nav";
import { Search } from "@/components/search";
import { githubUrl } from "@/lib/site";
import "./globals.css";

const bricolage = Bricolage_Grotesque({ variable: "--font-bricolage", subsets: ["latin"] });
const onest = Onest({ variable: "--font-onest", subsets: ["latin"] });
const jetbrains = JetBrains_Mono({ variable: "--font-jetbrains", subsets: ["latin"] });

export const metadata: Metadata = {
  title: { default: "Chain SDK", template: "%s | Chain SDK" },
  description:
    "Build desktop apps in TypeScript. Chain SDK gives you one typed API for native features, with a Rust core and a swappable runtime underneath."
};

export default function RootLayout({ children }: LayoutProps<"/">) {
  return (
    <html lang="en" className={clsx(bricolage.variable, onest.variable, jetbrains.variable, "antialiased")}>
      <body className="min-h-dvh font-sans">
        <header
          className={clsx(
            "sticky top-0 z-20",
            "bg-canvas/85 backdrop-blur",
            "border-b border-line"
          )}
        >
          <div className="mx-auto flex h-14 max-w-7xl items-center gap-4 px-4 sm:px-6">
            <MobileNav />
            <Link href="/" className="flex items-center gap-2.5">
              <ChainMark className="size-7" />
              <span className="font-display text-lg font-bold tracking-tight">
                Chain <span className="text-ink-muted font-medium">SDK</span>
              </span>
            </Link>
            <div className="ml-auto flex flex-1 items-center justify-end gap-4">
              <Search />
              <a href={githubUrl} className="hidden text-ink-muted text-sm hover:text-ink sm:inline">
                GitHub
              </a>
            </div>
          </div>
        </header>
        {children}
      </body>
    </html>
  );
}
