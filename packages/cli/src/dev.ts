import { execFileSync, spawn, type ChildProcessByStdio } from "node:child_process";
import fs from "node:fs";
import type { Readable } from "node:stream";

import { largeCacheHint } from "./clean.js";
import { DEFAULT_DEV_PORT, devPortConfig, freeDevPort, readBeforeDevCommand } from "./devPort.js";
import { chainCoreFeaturesOrExit } from "./features.js";
import {
  ansi,
  freshState,
  makeColor,
  printFailureSummary,
  processLine,
  type NativeOutputState
} from "./nativeOutput.js";
import {
  checkChainApp,
  resolveTauriBin,
  sharedDevTargetDir,
  taskkillPath,
  tauriEnv
} from "./nativeProject.js";
import { syncPermissionsOrExit } from "./permissions.js";

export async function dev(args: string[]): Promise<void> {
  const cwd = process.cwd();
  checkChainApp(cwd);

  const tauriBin = resolveTauriBin(cwd);
  if (!fs.existsSync(tauriBin)) {
    console.error("Error: Tauri CLI not found in node_modules/.bin — run `npm install` first.");
    process.exit(1);
  }

  const color = makeColor(Boolean(process.stdout.isTTY));
  const canReadKeys = Boolean(process.stdin.isTTY);

  console.log(color(ansi.bold + ansi.cyan, "⛓  chain dev") + "\n");

  const cacheHint = largeCacheHint(
    process.env.CARGO_TARGET_DIR || sharedDevTargetDir(),
    "chain clean --all"
  );
  if (cacheHint) console.log(color(ansi.gray, `  ${cacheHint}`) + "\n");

  let state: NativeOutputState = freshState();
  let verbose = false;
  let child: ChildProcessByStdio<null, Readable, Readable>;
  let exiting = false;
  // Bumped on every (re)spawn so a stale child's late stdout/exit events
  // (after `r` restarts it, or after quitting) can't clobber the current
  // state — each listener closes over the generation it was created for.
  let generation = 0;
  // True while we're deliberately killing the current child (restart or
  // quit) so its exit handler doesn't print a spurious "exited" message.
  let suppressExitMessage = false;

  /** Kills `target` and everything it spawned (`tauri dev` itself spawns
   * Vite and, once built, the app binary as separate child processes —
   * killing just the `tauri` pid leaves those running, which is why a
   * naive restart used to leave the old Vite dev server holding the port
   * and the old app window still open). `spawnTauri()` below spawns with
   * `detached: true` so the child is the leader of its own process group;
   * signaling the negative pid reaches that whole group on POSIX. Windows
   * has no process groups for this, so `taskkill /T` walks the tree
   * instead. Resolves once the child has actually exited, or after a
   * SIGKILL escalation if it ignores SIGTERM. */
  function killChildTree(target: ChildProcessByStdio<null, Readable, Readable>): Promise<void> {
    return new Promise((resolve) => {
      if (target.exitCode !== null || target.signalCode !== null) {
        resolve();
        return;
      }
      target.once("exit", () => resolve());
      const pid = target.pid;
      if (pid === undefined) {
        resolve();
        return;
      }
      if (process.platform === "win32") {
        try {
          execFileSync(taskkillPath(), ["/pid", String(pid), "/T", "/F"]);
        } catch {
          target.kill();
        }
        return;
      }
      try {
        process.kill(-pid, "SIGTERM");
      } catch {
        target.kill("SIGTERM");
      }
      setTimeout(() => {
        if (target.exitCode === null && target.signalCode === null) {
          try {
            process.kill(-pid, "SIGKILL");
          } catch {
            target.kill("SIGKILL");
          }
        }
      }, 3000);
    });
  }

  function handleChunk(chunk: Buffer): void {
    for (const rawLine of chunk.toString("utf8").split(/\r?\n/)) {
      if (verbose) {
        if (rawLine.trim()) console.log(`  ${color(ansi.dim, rawLine)}`);
      } else {
        processLine(rawLine, state, color);
      }
    }
  }

  /** `--config` moving the dev server and window off a taken 1420 —
   * another Chain app's `chain dev`, usually. Empty when 1420 is free. */
  async function devPortArgs(): Promise<string[]> {
    const port = await freeDevPort();
    if (port === DEFAULT_DEV_PORT) return [];
    const config = devPortConfig(readBeforeDevCommand(cwd), port);
    if (!config) {
      console.error(
        `Error: port ${DEFAULT_DEV_PORT} is in use, and tauri.conf.json's beforeDevCommand isn't an ` +
          "`npm run` script, so chain dev can't move the dev server to another port. Stop whatever holds it."
      );
      process.exit(1);
    }
    console.log(color(ansi.gray, `  port ${DEFAULT_DEV_PORT} is in use — using ${port}`) + "\n");
    return ["--config", config];
  }

  async function spawnTauri(): Promise<void> {
    state = freshState();
    suppressExitMessage = false;
    const gen = ++generation;
    // Re-read on every (re)start, so an edited declaration takes effect on "r".
    syncPermissionsOrExit(cwd);
    const portArgs = await devPortArgs();
    // --features chain-dev-inspector powers `chain inspect` and
    // chain-core/dev-trace its --trace — chain build passes neither, so
    // they never ship in a release binary.
    const features = ["chain-dev-inspector", "chain-core/dev-trace", ...chainCoreFeaturesOrExit(cwd)].join(",");
    child = spawn(tauriBin, ["dev", "--features", features, ...portArgs, ...args], {
      cwd,
      // A developer's own CARGO_TARGET_DIR still wins.
      env: { CARGO_TARGET_DIR: sharedDevTargetDir(), ...tauriEnv(cwd) },
      stdio: ["ignore", "pipe", "pipe"],
      // Leader of its own process group, so restart/quit can signal the
      // whole tree (Vite, the built app binary) instead of just this pid.
      detached: process.platform !== "win32"
    });
    child.stdout.on("data", (chunk: Buffer) => {
      if (gen === generation) handleChunk(chunk);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      if (gen === generation) handleChunk(chunk);
    });
    const pid = child.pid;
    child.on("exit", (code) => {
      // tauri dev can exit and leave Vite or the app running in its group
      // (a failed first start did) — they'd hold the port and a window.
      if (process.platform !== "win32" && pid !== undefined) {
        try {
          process.kill(-pid, "SIGTERM");
        } catch {
          // The group is already gone.
        }
      }
      if (exiting || suppressExitMessage || gen !== generation) return;
      state.native.status = code ? "error" : "stopped";
      const restartHint = canReadKeys ? " — press r to restart, q to quit" : "";
      if (code) printFailureSummary("tauri dev", code, state, color);
      const exitLine = `── tauri dev exited (code ${code ?? "unknown"})${restartHint} ──`;
      console.log(`  ${color(ansi.gray, exitLine)}`);
      if (!canReadKeys) process.exit(code ?? 1);
    });
  }

  function cleanup(): void {
    if (canReadKeys) {
      process.stdin.setRawMode(false);
      process.stdin.pause();
    }
  }

  async function quit(code: number): Promise<void> {
    exiting = true;
    suppressExitMessage = true;
    cleanup();
    await killChildTree(child);
    process.exit(code);
  }

  await spawnTauri();

  if (canReadKeys) {
    console.log(color(ansi.gray, "  r restart   v verbose   q quit") + "\n");
    process.stdin.setRawMode(true);
    process.stdin.resume();
    process.stdin.setEncoding("utf8");
    process.stdin.on("data", (key: string) => {
      if (key === "" || key === "q") {
        void quit(0);
        return;
      }
      if (key === "r") {
        suppressExitMessage = true;
        console.log(`  ${color(ansi.gray, "── restarting ──")}`);
        const previous = child;
        void killChildTree(previous).then(() => spawnTauri());
        return;
      }
      if (key === "v") {
        verbose = !verbose;
        console.log(
          color(
            ansi.gray,
            verbose ? "── verbose mode on — press v to go back ──" : "── verbose mode off ──"
          )
        );
      }
    });
  }

  process.on("SIGTERM", () => void quit(0));
  process.on("SIGINT", () => void quit(0));
  // Sent when the controlling terminal closes (window closed, tab killed,
  // SSH session dropped) — without a handler Node's default SIGHUP action
  // terminates us immediately, skipping killChildTree and orphaning the
  // detached tauri/Vite/app process tree.
  process.on("SIGHUP", () => void quit(0));
}
