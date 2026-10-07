import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/** Where the Tauri native project lives, hidden from day-to-day view (see
 * agent-docs/framework/command/README.md's `.chain/native` section) — a sibling of
 * `.chain/baseline`, not a subfolder of it. */
export function nativeProjectDir(cwd: string): string {
  return path.join(cwd, ".chain/native");
}

/** taskkill by absolute path, so a `taskkill` earlier on PATH can't stand in for it. */
export function taskkillPath(): string {
  return path.join(process.env.SystemRoot ?? String.raw`C:\Windows`, "System32", "taskkill.exe");
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

/** One Cargo target dir for every Chain app's `chain dev` builds on this
 * machine — the ~1 GB of compiled Tauri dependencies is identical across
 * apps, so each extra app only adds its own crate instead of another full
 * copy (same idea as Electron's shared Chromium download cache). A plain
 * build cache: deleting it only costs a rebuild. `chain build` keeps its
 * own target/ so release bundles land inside the app. */
export function sharedDevTargetDir(): string {
  if (process.platform === "darwin") {
    return path.join(os.homedir(), "Library", "Caches", "chain", "target");
  }
  if (process.platform === "win32") {
    const localAppData = process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local");
    return path.join(localAppData, "chain", "cache", "target");
  }
  const cacheHome = process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache");
  return path.join(cacheHome, "chain", "target");
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
