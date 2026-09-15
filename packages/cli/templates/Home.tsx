import { desktop } from "@chain/sdk";
import type { PlatformInfo } from "@chain/sdk";
import clsx from "clsx";
import { useEffect, useState } from "react";

import chainIcon from "../../asset/app-icon.svg";

type Status =
  { kind: "loading" } | { kind: "ready"; info: PlatformInfo } | { kind: "error"; message: string };

export default function Home() {
  const [status, setStatus] = useState<Status>({ kind: "loading" });

  useEffect(() => {
    desktop.platform
      .getInfo()
      .then((info) => setStatus({ kind: "ready", info }))
      .catch((error: unknown) =>
        setStatus({
          kind: "error",
          message: typeof error === "string" ? error : JSON.stringify(error, null, 2)
        })
      );
  }, []);

  return (
    <main className="mx-auto flex max-w-lg flex-col items-center gap-4 px-4 py-16 text-center">
      <img src={chainIcon} width={72} height={72} alt="Chain" className="rounded-xl" />
      <h1 className="text-xl font-semibold">Welcome to your Chain app</h1>
      <p className="text-sm text-chain-navy/70 dark:text-chain-cream/70">
        Chain is a cross-platform desktop framework: a shared React + TypeScript UI, a Rust
        coordination layer (<span className="font-medium">Chain Core</span>), and a native runtime
        underneath (<span className="font-medium">Tauri</span> today). Your app calls one stable API
        — <code className="font-mono">desktop.*</code> — and Chain handles the platform differences
        underneath it.
      </p>
      <p className="text-sm text-chain-navy/60 dark:text-chain-cream/60">
        The card below calls <code className="font-mono">desktop.platform.getInfo()</code> through{" "}
        <code className="font-mono">@chain/sdk</code>, all the way down to Rust and back, to prove
        this app can actually reach the framework.
      </p>

      <section
        className={clsx(
          "w-full rounded-lg border p-4 text-left",
          status.kind === "loading" &&
            "border-chain-navy/15 text-chain-navy/60 dark:border-chain-cream/15 dark:text-chain-cream/60",
          status.kind === "ready" && "border-chain-lime",
          status.kind === "error" && "border-red-500/60 text-red-600 dark:text-red-400"
        )}
      >
        {status.kind === "loading" && <span>Checking platform…</span>}
        {status.kind === "ready" && (
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 font-mono text-sm">
            <dt className="text-chain-navy/60 dark:text-chain-cream/60">OS</dt>
            <dd className="text-right">{status.info.os}</dd>
            <dt className="text-chain-navy/60 dark:text-chain-cream/60">Arch</dt>
            <dd className="text-right">{status.info.arch}</dd>
            <dt className="text-chain-navy/60 dark:text-chain-cream/60">Runtime</dt>
            <dd className="text-right">{status.info.runtimeVersion}</dd>
          </dl>
        )}
        {status.kind === "error" && (
          <pre className="whitespace-pre-wrap font-mono text-xs">{status.message}</pre>
        )}
      </section>
    </main>
  );
}
