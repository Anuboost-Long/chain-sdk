import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { largeCacheHint } from "./clean.js";
import { chainCoreFeaturesOrExit } from "./features.js";
import { ansi, freshState, makeColor, printFailureSummary, processLine } from "./nativeOutput.js";
import {
  checkChainApp,
  nativeProjectDir,
  resolveTauriBin,
  taskkillPath,
  tauriEnv
} from "./nativeProject.js";
import { buildPermissionArgs, syncPermissionsOrExit } from "./permissions.js";

export async function build(args: string[]): Promise<void> {
  const cwd = process.cwd();
  checkChainApp(cwd);

  const tauriBin = resolveTauriBin(cwd);
  if (!fs.existsSync(tauriBin)) {
    console.error("Error: Tauri CLI not found in node_modules/.bin — run `npm install` first.");
    process.exit(1);
  }

  const color = makeColor(Boolean(process.stdout.isTTY));
  console.log(color(ansi.bold + ansi.cyan, "⛓  chain build") + "\n");

  const permissionArgs = buildPermissionArgs(syncPermissionsOrExit(cwd));
  const features = chainCoreFeaturesOrExit(cwd);
  const featureArgs = features.length > 0 ? ["--features", features.join(",")] : [];
  const state = freshState();
  const child = spawn(tauriBin, ["build", ...permissionArgs, ...featureArgs, ...args], {
    cwd,
    env: tauriEnv(cwd),
    stdio: ["ignore", "pipe", "pipe"],
    // Leader of its own process group, so an aborted build (Ctrl+C) can
    // kill the whole cargo/rustc tree, not just this one pid — same
    // reasoning as chain dev's restart handling.
    detached: process.platform !== "win32"
  });

  function handleChunk(chunk: Buffer): void {
    for (const rawLine of chunk.toString("utf8").split(/\r?\n/)) {
      processLine(rawLine, state, color);
    }
  }
  child.stdout.on("data", handleChunk);
  child.stderr.on("data", handleChunk);

  function killTree(signal: NodeJS.Signals): void {
    const pid = child.pid;
    if (pid === undefined) return;
    if (process.platform === "win32") {
      try {
        execFileSync(taskkillPath(), ["/pid", String(pid), "/T", "/F"]);
      } catch {
        child.kill();
      }
      return;
    }
    try {
      process.kill(-pid, signal);
    } catch {
      child.kill(signal);
    }
  }

  process.on("SIGINT", () => killTree("SIGTERM"));
  process.on("SIGTERM", () => killTree("SIGTERM"));

  const code: number = await new Promise((resolve) => child.on("exit", (c) => resolve(c ?? 1)));
  if (code === 0) {
    console.log(`\n${color(ansi.green, "● Build finished")}`);
    const cacheHint = largeCacheHint(path.join(nativeProjectDir(cwd), "target"), "chain clean");
    if (cacheHint) console.log(color(ansi.gray, cacheHint));
  } else {
    printFailureSummary("Build", code, state, color);
  }
  process.exit(code);
}
