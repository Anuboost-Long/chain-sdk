import clsx from "clsx";
import Link from "next/link";
import { codeToHtml } from "shiki";

import {
  capabilityNames,
  readCapability,
  readContractSource,
  type CapabilityMeta,
  type SupportLevel
} from "@/lib/capabilities";
import { chainDark, chainLight } from "@/lib/code-themes";

const supportLabel: Record<SupportLevel, string> = {
  tested: "Tested",
  experimental: "Experimental",
  partial: "Partial",
  "not-started": "Not yet",
  "not-planned": "Not planned"
};

const platformLabel = { macos: "macOS", windows: "Windows", linux: "Linux" } as const;

const statusLabel: Record<CapabilityMeta["status"], string> = {
  draft: "Draft",
  experimental: "Experimental",
  stable: "Stable"
};

function SupportValue({ level }: Readonly<{ level: SupportLevel }>) {
  const available = level === "tested" || level === "experimental" || level === "partial";
  return (
    <span className="inline-flex items-center gap-2">
      <span
        aria-hidden="true"
        className={clsx("size-2 rounded-full", available ? "bg-lime" : "border border-ink-faint")}
      />
      <span className={available ? "text-ink" : "text-ink-muted"}>{supportLabel[level]}</span>
    </span>
  );
}

/** Where a capability runs, read from its component.json at build time. */
export function PlatformSupport({ name }: Readonly<{ name: string }>) {
  const meta = readCapability(name);
  return (
    <dl className="not-prose my-6 grid grid-cols-2 gap-x-8 gap-y-3 text-sm sm:flex sm:flex-wrap">
      {(Object.keys(platformLabel) as (keyof typeof platformLabel)[]).map((platform) => (
        <div key={platform}>
          <dt className="text-ink-faint">{platformLabel[platform]}</dt>
          <dd className="mt-0.5">
            <SupportValue level={meta.platforms[platform]} />
          </dd>
        </div>
      ))}
      <div>
        <dt className="text-ink-faint">API status</dt>
        <dd className="mt-0.5">
          <Link href="/concepts/#status-levels" className="text-ink underline decoration-ink-faint underline-offset-3">
            {statusLabel[meta.status]}
          </Link>
        </dd>
      </div>
    </dl>
  );
}

export async function highlight(code: string, lang: string) {
  return codeToHtml(code, {
    lang,
    themes: { dark: chainDark, light: chainLight },
    defaultColor: false
  });
}

/** The capability's contract.ts, verbatim, so the reference is the real type definitions. */
export async function ContractSource({ name }: Readonly<{ name: string }>) {
  const html = await highlight(readContractSource(name), "ts");
  return (
    <figure className="my-5">
      <figcaption
        className={clsx(
          "bg-raised",
          "border border-b-0 border-line rounded-t-[10px]",
          "text-ink-muted font-mono text-xs",
          "px-4.5 py-2"
        )}
      >
        capabilities/{name}/contract.ts
      </figcaption>
      <div className="[&_pre]:rounded-t-none" dangerouslySetInnerHTML={{ __html: html }} />
    </figure>
  );
}

const capabilityPages: Record<string, { title: string; api: string; href: string; summary: string }> = {
  platform: {
    title: "Platform",
    api: "desktop.platform",
    href: "/capabilities/platform/",
    summary: "OS, CPU architecture, and runtime version."
  },
  storage: {
    title: "Storage",
    api: "desktop.storage",
    href: "/capabilities/storage/",
    summary: "The app's own SQLite database, with migrations."
  },
  files: {
    title: "Files",
    api: "desktop.files",
    href: "/capabilities/files/",
    summary: "Store and show binary files by an opaque reference."
  },
  http: {
    title: "Http",
    api: "desktop.http",
    href: "/capabilities/http/",
    summary: "GET any URL from native code, free of CORS."
  },
  "agent-server": {
    title: "Agent server",
    api: "desktop.agentServer",
    href: "/capabilities/agent-server/",
    summary: "A localhost HTTP endpoint external agents can call."
  },
  "process-runner": {
    title: "Process runner",
    api: "desktop.processRunner",
    href: "/capabilities/process-runner/",
    summary: "Run a program and stream its output live."
  },
  folders: {
    title: "Folders",
    api: "desktop.folders",
    href: "/capabilities/folders/",
    summary: "Real files in folders the user chose."
  },
  terminal: {
    title: "Terminal",
    api: "desktop.terminal",
    href: "/capabilities/terminal/",
    summary: "Interactive programs that survive a reload."
  },
  ports: {
    title: "Ports",
    api: "desktop.ports",
    href: "/capabilities/ports/",
    summary: "Whether a local TCP port is free."
  },
  attention: {
    title: "Attention",
    api: "desktop.attention",
    href: "/capabilities/attention/",
    summary: "Notifications, focus, and a Dock bounce."
  },
  "page-zoom": {
    title: "Page zoom",
    api: "desktop.pageZoom",
    href: "/capabilities/page-zoom/",
    summary: "Browser-style zoom with real reflow."
  },
  "keep-awake": {
    title: "Keep awake",
    api: "desktop.keepAwake",
    href: "/capabilities/keep-awake/",
    summary: "Stop idle sleep while your app works."
  }
};

const capabilityOrder = ["platform", "storage", "files", "http", "agent-server", "process-runner", "folders", "terminal", "ports", "attention", "page-zoom", "keep-awake"];

/** Every capability with its macOS/Windows/Linux support, one row each. */
export function CapabilityTable() {
  const names = capabilityNames().sort((a, b) => capabilityOrder.indexOf(a) - capabilityOrder.indexOf(b));
  return (
    <div className="not-prose my-6 overflow-x-auto">
      <table className="w-full min-w-160 border-collapse text-left text-sm">
        <thead>
          <tr className="border-b border-line text-ink-faint">
            <th className="py-2 pr-4 font-medium">Capability</th>
            <th className="py-2 pr-4 font-medium">What it does</th>
            <th className="py-2 pr-4 font-medium">macOS</th>
            <th className="py-2 pr-4 font-medium">Windows</th>
            <th className="py-2 font-medium">Linux</th>
          </tr>
        </thead>
        <tbody>
          {names.map((name) => {
            const meta = readCapability(name);
            const page = capabilityPages[name];
            return (
              <tr key={name} className="border-b border-line align-top">
                <td className="py-3 pr-4">
                  {page ? (
                    <Link href={page.href} className="font-medium text-ink hover:text-link">
                      {page.title}
                    </Link>
                  ) : (
                    <span className="font-medium">{name}</span>
                  )}
                  <code className="mt-0.5 block font-mono text-ink-faint text-xs">{page?.api}</code>
                </td>
                <td className="py-3 pr-4 text-ink-muted">{page?.summary ?? meta.description}</td>
                <td className="py-3 pr-4 whitespace-nowrap">
                  <SupportValue level={meta.platforms.macos} />
                </td>
                <td className="py-3 pr-4 whitespace-nowrap">
                  <SupportValue level={meta.platforms.windows} />
                </td>
                <td className="py-3 whitespace-nowrap">
                  <SupportValue level={meta.platforms.linux} />
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
