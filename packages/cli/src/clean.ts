import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { checkChainApp, nativeProjectDir, sharedDevTargetDir } from "./nativeProject.js";

// Past this, `chain dev`/`chain build` print a one-line hint pointing at
// `chain clean`. Cargo never deletes stale artifacts on its own (no stable
// target-dir GC as of cargo 1.98), so caches only ever grow.
const LARGE_CACHE_BYTES = 8 * 1024 ** 3;

function directorySize(dir: string): number {
  let total = 0;
  const pending = [dir];
  while (pending.length) {
    const current = pending.pop()!;
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(current, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(full);
      else if (entry.isFile()) total += fs.statSync(full, { throwIfNoEntry: false })?.size ?? 0;
    }
  }
  return total;
}

function formatSize(bytes: number): string {
  return bytes >= 1024 ** 3 ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : `${Math.round(bytes / 1024 ** 2)} MB`;
}

/** The one-line "your cache is huge" hint, or nothing. */
export function largeCacheHint(dir: string, fix: string): string | undefined {
  if (!fs.existsSync(dir)) return;
  const size = directorySize(dir);
  if (size < LARGE_CACHE_BYTES) return;
  return `Build cache at ${dir} is ${formatSize(size)}. Run \`${fix}\` to reclaim it; the next build recompiles.`;
}

// rustup's install location (what `chain doctor` installs), so which cargo
// runs doesn't depend on PATH order; PATH is only the fallback for
// non-rustup installs such as Homebrew's.
function cargoBin(): string {
  const cargoHome = process.env.CARGO_HOME || path.join(os.homedir(), ".cargo");
  const bin = path.join(cargoHome, "bin", process.platform === "win32" ? "cargo.exe" : "cargo");
  return fs.existsSync(bin) ? bin : "cargo";
}

function packageName(cwd: string): string {
  const manifest = fs.readFileSync(path.join(nativeProjectDir(cwd), "Cargo.toml"), "utf8");
  const name = /^name\s*=\s*"([^"]+)"/m.exec(manifest)?.[1];
  if (!name) throw new Error("couldn't find the package name in .chain/native/Cargo.toml");
  return name;
}

/**
 * `chain clean` — this app's release builds (its own `.chain/native/target`)
 * plus this app's own crate in the shared dev cache, keeping the compiled
 * dependencies other Chain apps reuse. `--all` deletes the whole shared dev
 * cache instead, for every app. Nothing here runs unless the developer
 * types the command.
 */
export function clean(args: string[]): void {
  const cwd = process.cwd();
  const all = args.includes("--all");
  const shared = process.env.CARGO_TARGET_DIR || sharedDevTargetDir();

  if (all) {
    if (!fs.existsSync(shared)) {
      console.log(`Nothing to clean — ${shared} doesn't exist.`);
      return;
    }
    const size = directorySize(shared);
    fs.rmSync(shared, { recursive: true, force: true });
    console.log(`Removed the shared dev build cache (${formatSize(size)}) at ${shared}.`);
    console.log("Every Chain app's next `chain dev` recompiles its dependencies once.");
    return;
  }

  checkChainApp(cwd);
  const manifest = path.join(nativeProjectDir(cwd), "Cargo.toml");
  const ownTarget = path.join(nativeProjectDir(cwd), "target");
  const before = fs.existsSync(ownTarget) ? directorySize(ownTarget) : 0;

  // Without CARGO_TARGET_DIR, cargo cleans the native project's own target/.
  const { CARGO_TARGET_DIR: _ignored, ...envWithoutTarget } = process.env;
  execFileSync(cargoBin(), ["clean", "--manifest-path", manifest], { stdio: "ignore", env: envWithoutTarget });
  console.log(`Removed this app's release builds (${formatSize(before)}) at ${ownTarget}.`);

  if (fs.existsSync(shared)) {
    execFileSync(cargoBin(), ["clean", "--manifest-path", manifest, "-p", packageName(cwd)], {
      stdio: "ignore",
      env: { ...process.env, CARGO_TARGET_DIR: shared }
    });
    console.log(`Removed this app's own crate from the shared dev cache at ${shared}.`);
    console.log("Run `chain clean --all` to clear the shared dependencies too.");
  }
}
