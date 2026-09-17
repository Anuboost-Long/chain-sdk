import fs from "node:fs";
import os from "node:os";
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

/**
 * Where the app's SQLite file lives — the exact path Tauri's own
 * `app.path().app_data_dir()` resolves to (verified against tauri 2.11.5's
 * and dirs 6.0.0's source: `dirs::data_dir().join(identifier)`), joined
 * with `app.db` the way `templates/lib.rs`'s `get_db()` opens it. `chain
 * database` needs this to operate on the same file the running app uses,
 * without going through Tauri (which only exists inside a running app).
 */
export function resolveDbPath(cwd: string): string {
  const confPath = path.join(nativeProjectDir(cwd), "tauri.conf.json");
  const { identifier } = JSON.parse(fs.readFileSync(confPath, "utf8")) as { identifier: string };

  let baseDir: string;
  if (process.platform === "darwin") {
    baseDir = path.join(os.homedir(), "Library", "Application Support", identifier);
  } else if (process.platform === "win32") {
    if (!process.env.APPDATA) {
      console.error("Error: %APPDATA% isn't set — can't resolve the app's data directory.");
      process.exit(1);
    }
    baseDir = path.join(process.env.APPDATA, identifier);
  } else {
    const dataHome = process.env.XDG_DATA_HOME || path.join(os.homedir(), ".local", "share");
    baseDir = path.join(dataHome, identifier);
  }
  return path.join(baseDir, "app.db");
}
