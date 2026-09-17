import { execFileSync, spawn } from "node:child_process";
import fs from "node:fs";

import { checkChainApp, resolveTauriBin, tauriEnv } from "./nativeProject.js";
import { ansi, freshState, makeColor, processLine } from "./nativeOutput.js";

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

  const state = freshState();
  const child = spawn(tauriBin, ["build", ...args], {
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
        execFileSync("taskkill", ["/pid", String(pid), "/T", "/F"]);
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
  process.exit(code);
}
