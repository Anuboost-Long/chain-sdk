import clsx from "clsx";
import Link from "next/link";

import { CapabilityTable, highlight } from "@/components/capability";
import { LayerTrace } from "@/components/layer-trace";
import { githubUrl } from "@/lib/site";

const steps = [
  { title: "Check your toolchain", code: "chain doctor", note: "Finds or installs Rust. Only builders need it, never your users." },
  { title: "Create an app", code: "chain init my-app", note: "React, routing, Tailwind, and the SDK, already wired up." },
  { title: "Run it", code: "cd my-app && npm run dev", note: "A native window with hot reload and readable output." },
  {
    title: "Call the OS",
    code: 'import { desktop } from "@chain/sdk";\n\nconst info = await desktop.platform.getInfo();',
    note: "Typed, and identical on every platform Chain supports."
  }
];

export default async function Home() {
  const stepHtml = await Promise.all(
    steps.map((step, index) => highlight(step.code, index === steps.length - 1 ? "ts" : "bash"))
  );

  return (
    <>
      <main className="mx-auto max-w-7xl px-4 sm:px-6">
        <section className="grid items-center gap-14 py-16 lg:grid-cols-[minmax(0,1fr)_minmax(0,30rem)] lg:py-24">
          <div>
            <h1 className="max-w-[16ch] font-display text-[clamp(2.5rem,6vw,4rem)] font-bold leading-[1.02] tracking-[-0.03em]">
              Build desktop apps in TypeScript. Chain handles the native side.
            </h1>
            <p className="mt-6 max-w-[54ch] text-ink-muted text-lg leading-relaxed">
              One typed API for storage, files, processes, and local servers. A Rust core does the native work, and the
              runtime underneath can be swapped without touching your app code.
            </p>
            <div className="mt-9 flex flex-wrap gap-3">
              <Link
                href="/getting-started/"
                className={clsx(
                  "inline-flex h-11 items-center",
                  "bg-lime",
                  "rounded-lg",
                  "text-on-lime font-semibold",
                  "px-5",
                  "hover:brightness-105"
                )}
              >
                Get started
              </Link>
              <Link
                href="/capabilities/"
                className={clsx(
                  "inline-flex h-11 items-center",
                  "border border-line rounded-lg",
                  "text-ink font-medium",
                  "px-5",
                  "hover:border-ink-faint"
                )}
              >
                Browse capabilities
              </Link>
            </div>
          </div>
          <LayerTrace />
        </section>

        <section className="border-t border-line py-16">
          <h2 className="font-display text-3xl font-bold tracking-tight">From nothing to a running app</h2>
          <ol className="mt-10 grid gap-x-10 gap-y-10 md:grid-cols-2">
            {steps.map((step, index) => (
              <li key={step.title} className="grid grid-cols-[2rem_minmax(0,1fr)] gap-x-3">
                <span className="font-display text-lime text-xl font-bold [@media(prefers-color-scheme:light)]:text-link">
                  {index + 1}
                </span>
                <div className="min-w-0">
                  <h3 className="font-display text-xl font-semibold">{step.title}</h3>
                  <p className="mt-1 text-ink-muted">{step.note}</p>
                  <div className="mt-4" dangerouslySetInnerHTML={{ __html: stepHtml[index] }} />
                </div>
              </li>
            ))}
          </ol>
        </section>

        <section className="border-t border-line py-16">
          <div className="flex flex-wrap items-end justify-between gap-4">
            <h2 className="font-display text-3xl font-bold tracking-tight">What your app can do today</h2>
            <Link href="/concepts/#status-levels" className="text-link underline underline-offset-3">
              How support levels work
            </Link>
          </div>
          <CapabilityTable />
        </section>
      </main>
      <footer className="border-t border-line">
        <div className="mx-auto flex max-w-7xl flex-wrap justify-between gap-4 px-4 py-8 text-ink-muted text-sm sm:px-6">
          <p>Chain SDK is pre-1.0. Every API is a Draft and may still change.</p>
          <a href={githubUrl} className="hover:text-ink">
            Source on GitHub
          </a>
        </div>
      </footer>
    </>
  );
}
