import fs from "node:fs";
import path from "node:path";

/** Where the Tauri native project lives, hidden from day-to-day view (see
 * agent-docs/framework/command/README.md's `.chain/native` section) — a sibling of
 * `.chain/baseline`, not a subfolder of it. */
export function nativeProjectDir(cwd: string): string {
  return path.join(cwd, ".chain/native");
}

export function resolveTauriBin(cwd: string): string {
  const bin = process.platform === "win32" ? "tauri.cmd" : "tauri";
  return path.join(cwd, "node_modules", ".bin", bin);
}

/** Where the dev inspector (see dev_inspector.rs) writes its port+token —
 * `chain inspect` reads the same path. Lands inside the native project's
 * own `target/`, which every scaffolded app already gitignores. */
export function inspectorInfoPath(cwd: string): string {
  return path.join(nativeProjectDir(cwd), "target", "chain-inspector.json");
}

/** Exits the process with a clear error if `cwd` isn't a Chain app, or is
 * one that still has the pre-`.chain/native` layout (needs `chain update`
 * to migrate before `chain dev`/`chain build` can find its native project). */
export function checkChainApp(cwd: string): void {
  if (!fs.existsSync(path.join(cwd, "package.json"))) {
    console.error("Error: this doesn't look like a Chain app (no package.json here).");
    process.exit(1);
  }
  if (fs.existsSync(nativeProjectDir(cwd))) return;
  if (fs.existsSync(path.join(cwd, "src-tauri"))) {
    console.error(
      "Error: this app still has the old src-tauri layout — run `chain update` first to migrate " +
        "it to .chain/native."
    );
  } else {
    console.error("Error: this doesn't look like a Chain app (no .chain/native here).");
  }
  process.exit(1);
}

/** Env for spawning `tauri dev`/`tauri build`: TAURI_APP_PATH points the
 * Tauri CLI at the hidden `.chain/native` project instead of the
 * `src-tauri`-next-to-cwd convention it looks for by default. */
export function tauriEnv(cwd: string): NodeJS.ProcessEnv {
  return {
    ...process.env,
    CARGO_TERM_COLOR: "always",
    FORCE_COLOR: "1",
    TAURI_APP_PATH: nativeProjectDir(cwd)
  };
}
