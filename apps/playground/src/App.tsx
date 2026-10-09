import { desktop } from "@chain/sdk";
import type { PlatformInfo } from "@chain/sdk";
import { useEffect, useState } from "react";

import chainIcon from "../../../asset/app-icon.svg";
import "./App.css";
import { dismissLaunchScreen } from "./launchScreen";

type Status =
  { kind: "loading" } | { kind: "ready"; info: PlatformInfo } | { kind: "error"; message: string };

// The layers this app exists to prove — see docs/ARCHITECTURE.md. Listed
// together because a single getInfo() call can't tell us which layer
// failed, only that the whole round trip did or didn't work.
const LAYERS = ["React", "Chain SDK", "Chain Core (Rust)", "Tauri runtime"] as const;

function App() {
  const [status, setStatus] = useState<Status>({ kind: "loading" });

  useEffect(() => {
    if (status.kind !== "loading") void dismissLaunchScreen();
  }, [status.kind]);

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
    <>
      {/* desktop.window: package.json's "chain.window" runs the page under
          the title bar; this bar drags the window and leaves room for its buttons. */}
      <div className="title-bar" data-chain-drag-region>
        <span>Drag here</span>
        <button type="button">Still clickable</button>
      </div>
      <main className="playground">
        <header>
          <img src={chainIcon} width="32" height="32" alt="" />
          <h1>Chain Playground</h1>
          <p className="subtitle">
            The framework's internal proof app — not a real product. See <code>AGENTS.md</code> before
            adding anything here.
          </p>
        </header>

        <ol className={`layers layers--${status.kind}`}>
          {LAYERS.map((layer) => (
            <li key={layer}>{layer}</li>
          ))}
        </ol>

        <section className={`result result--${status.kind}`}>
          {status.kind === "loading" && <span>Calling desktop.platform.getInfo()…</span>}
          {status.kind === "ready" && <PlatformTable info={status.info} />}
          {status.kind === "error" && <pre>{status.message}</pre>}
        </section>
      </main>
    </>
  );
}

function PlatformTable({ info }: { readonly info: PlatformInfo }) {
  return (
    <dl className="info">
      <dt>OS</dt>
      <dd>{info.os}</dd>
      <dt>Arch</dt>
      <dd>{info.arch}</dd>
      <dt>Runtime</dt>
      <dd>{info.runtimeVersion}</dd>
    </dl>
  );
}

export default App;
