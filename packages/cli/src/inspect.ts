import { execFile, execFileSync, execSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import readline from "node:readline";
import { promisify } from "node:util";

import { confirm } from "./doctor.js";
import { checkChainApp, inspectorInfoPath } from "./nativeProject.js";

const execFileAsync = promisify(execFile);

interface InspectorInfo {
  port: number;
  token: string;
  pid: number;
}

interface WindowRect {
  x: number;
  y: number;
  width: number;
  height: number;
  scaleFactor: number;
}

interface InspectorResponse {
  id: number;
  ok: boolean;
  result?: string;
  error?: string;
}

async function waitForInfo(infoPath: string, timeoutMs: number): Promise<InspectorInfo> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (fs.existsSync(infoPath)) {
      try {
        return JSON.parse(fs.readFileSync(infoPath, "utf8")) as InspectorInfo;
      } catch {
        // File mid-write on the Rust side — retry.
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error(
    `No dev inspector found at ${infoPath} — is \`chain dev\` running in this app? ` +
      "(chain-dev-inspector is only active while chain dev runs, never chain build.)"
  );
}

// All of click/click-text/text/wait/type are just JS snippets sent through
// the one `eval` primitive the bridge exposes — same shape as the Electron
// driver pattern's page.evaluate() wrappers.
const js = {
  click: (sel: string) =>
    `(function(){var el=document.querySelector(${JSON.stringify(sel)});if(!el)return 'NOT_FOUND';el.click();return 'OK';})()`,
  clickText: (text: string) =>
    `(function(){var t=${JSON.stringify(text)};var els=[...document.querySelectorAll('button,a,[role="button"]')];` +
    `var el=els.find(e=>e.textContent.trim()===t)||els.find(e=>e.textContent.includes(t));` +
    `if(!el)return 'NOT_FOUND';el.click();return 'OK: '+el.tagName;})()`,
  text: (sel?: string) =>
    `(function(){var el=${sel ? `document.querySelector(${JSON.stringify(sel)})` : "document.body"};return el?el.innerText:null;})()`,
  type: (sel: string, text: string) =>
    `(function(){var el=document.querySelector(${JSON.stringify(sel)});if(!el)return 'NOT_FOUND';` +
    `el.focus();el.value=${JSON.stringify(text)};el.dispatchEvent(new Event('input',{bubbles:true}));return 'OK';})()`,
  has: (sel: string) => `document.querySelector(${JSON.stringify(sel)}) !== null`
};

function formatResult(res: InspectorResponse): string {
  if (!res.ok) return `ERROR: ${res.error ?? "unknown error"}`;
  if (res.result === undefined) return "null";
  try {
    return JSON.stringify(JSON.parse(res.result));
  } catch {
    return res.result;
  }
}

function splitArgs(rest: string): string[] {
  return rest.split(/\s+/).filter(Boolean);
}

// Naive whitespace split, so a selector containing a space needs `eval`
// instead — acceptable for v1, matches how simple the rest of this REPL's
// argument parsing already is.
function extractSelector(tokens: string[]): { rest: string[]; selector?: string } {
  const idx = tokens.indexOf("--selector");
  if (idx === -1) return { rest: tokens };
  return { rest: [...tokens.slice(0, idx), ...tokens.slice(idx + 2)], selector: tokens[idx + 1] };
}

function hasCliclick(): boolean {
  try {
    execFileSync("cliclick", ["-V"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function requireMacOS(feature: string): void {
  if (process.platform !== "darwin") {
    throw new Error(
      `${feature} is macOS-only for now — see agent-docs/framework/command/README.md's non-goals.`
    );
  }
}

export async function inspect(args: string[]): Promise<void> {
  const cwd = process.cwd();
  checkChainApp(cwd);
  const infoPath = inspectorInfoPath(cwd);

  console.log("⛓  chain inspect — waiting for dev inspector...");
  const info = await waitForInfo(infoPath, 10_000);

  const socket = net.connect(info.port, "127.0.0.1");
  await new Promise<void>((resolve, reject) => {
    socket.once("connect", () => resolve());
    socket.once("error", reject);
  });

  let nextId = 1;
  const pending = new Map<number, (res: InspectorResponse) => void>();
  const lines = readline.createInterface({ input: socket });
  lines.on("line", (line) => {
    if (!line.trim()) return;
    let msg: InspectorResponse;
    try {
      msg = JSON.parse(line) as InspectorResponse;
    } catch {
      return;
    }
    const resolve = pending.get(msg.id);
    if (resolve) {
      pending.delete(msg.id);
      resolve(msg);
    }
  });

  function send(msg: Record<string, unknown>, timeoutMs = 10_000): Promise<InspectorResponse> {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        pending.delete(id);
        reject(new Error("timed out waiting for response"));
      }, timeoutMs);
      pending.set(id, (msg2) => {
        clearTimeout(timer);
        resolve(msg2);
      });
      socket.write(`${JSON.stringify({ id, token: info.token, ...msg })}\n`);
    });
  }

  function evalCode(code: string, timeoutMs = 10_000): Promise<InspectorResponse> {
    return send({ cmd: "eval", code }, timeoutMs);
  }

  async function getRect(): Promise<WindowRect> {
    const res = await send({ cmd: "rect" });
    if (!res.ok || res.result === undefined) throw new Error(res.error ?? "failed to get window rect");
    return JSON.parse(res.result) as WindowRect;
  }

  interface ViewportRegion {
    x: number;
    y: number;
    width: number;
    height: number;
  }

  // The bridge only reports physical-pixel window geometry (dev_inspector.rs
  // has no notion of "points") — screencapture/cliclick both expect the
  // point-based screen coordinate space (confirmed empirically: an N-point
  // -R rect produces an N*scaleFactor-pixel image), so this divides by
  // scaleFactor once at the window origin. getBoundingClientRect() is
  // already in CSS px, which equals points 1:1 — no second conversion.
  //
  // Tauri's inner_position()/inner_size() (what `rect` reports) measure the
  // window's content view, NOT the WKWebView's actual viewport — on macOS
  // there's a real gap between them for the title-bar/traffic-light area
  // even with an overlay title bar (confirmed empirically against a real
  // window: window.innerHeight read 32pt less than rect's height implied).
  // Hardcoding that as a constant would be wrong for any other title-bar
  // style, so it's measured fresh every call via window.innerWidth/Height
  // instead of assumed.
  async function viewportRegion(selector?: string): Promise<ViewportRegion> {
    const rect = await getRect();
    const res = await evalCode(
      `(function(){var el=${selector ? `document.querySelector(${JSON.stringify(selector)})` : "null"};` +
        "var r=el?el.getBoundingClientRect():null;" +
        "return {innerWidth:window.innerWidth,innerHeight:window.innerHeight," +
        "element:r?{left:r.left,top:r.top,width:r.width,height:r.height}:null};})()"
    );
    if (!res.ok || !res.result) throw new Error(res.error ?? "eval failed");
    const viewport = JSON.parse(res.result) as {
      innerWidth: number;
      innerHeight: number;
      element: { left: number; top: number; width: number; height: number } | null;
    };
    if (selector && !viewport.element) throw new Error(`selector not found: ${selector}`);

    const winPointWidth = rect.width / rect.scaleFactor;
    const winPointHeight = rect.height / rect.scaleFactor;
    const leftInset = winPointWidth - viewport.innerWidth;
    const topInset = winPointHeight - viewport.innerHeight;

    const region: ViewportRegion = {
      x: rect.x / rect.scaleFactor + leftInset,
      y: rect.y / rect.scaleFactor + topInset,
      width: winPointWidth - leftInset,
      height: winPointHeight - topInset
    };
    if (viewport.element) {
      region.x += viewport.element.left;
      region.y += viewport.element.top;
      region.width = viewport.element.width;
      region.height = viewport.element.height;
    }
    return region;
  }

  async function screenshotRegion(outPath: string, selector?: string): Promise<string> {
    requireMacOS("screenshot");
    const { x, y, width, height } = await viewportRegion(selector);
    const region = [x, y, width, height].map((n) => Math.round(n)).join(",");
    await execFileAsync("screencapture", ["-x", "-R", region, outPath]);
    const stat = await fs.promises.stat(outPath).catch(() => null);
    if (!stat || stat.size < 1024) {
      throw new Error(
        "screenshot looks empty or too small — does this terminal have Screen Recording " +
          "permission? (System Settings → Privacy & Security → Screen Recording)"
      );
    }
    return `saved: ${outPath} (${stat.size} bytes)`;
  }

  async function performDrag(x1: number, y1: number, x2: number, y2: number, selector?: string): Promise<string> {
    requireMacOS("drag");
    if (!hasCliclick()) {
      const install = await confirm(
        "cliclick is required for real OS-level drag input (not JS dispatchEvent). Install it now via `brew install cliclick`?"
      );
      if (!install) throw new Error("cliclick not installed — see https://github.com/BlueM/cliclick");
      execSync("brew install cliclick", { stdio: "inherit" });
    }
    // A synthetic mousedown on a window that isn't frontmost (the common
    // case — an agent's own terminal has focus, not the app it's driving)
    // just raises the window instead of registering as a real drag start.
    // Activating first makes the drag land on the actual click-and-drag
    // gesture instead of a focus-change.
    await execFileAsync("osascript", [
      "-e",
      `tell application "System Events" to set frontmost of (first process whose unix id is ${info.pid}) to true`
    ]).catch(() => {
      // Best-effort — if System Events/Automation permission isn't granted
      // yet, fall through and let the drag attempt (and likely fail
      // obviously) rather than blocking on a second permission gate here.
    });
    const { x, y } = await viewportRegion(selector);
    const ax1 = Math.round(x + x1);
    const ay1 = Math.round(y + y1);
    const ax2 = Math.round(x + x2);
    const ay2 = Math.round(y + y2);
    // dm: (drag-move), not m: (plain move) — m: emits a mouseMoved event,
    // which native text/drag selection ignores; dm: emits the mouseDragged
    // event apps actually listen for while the button is held. Confirmed
    // by testing both directly: m: never selected text, dm: did.
    await execFileAsync("cliclick", [`dd:${ax1},${ay1}`, "w:80", `dm:${ax2},${ay2}`, "w:80", `du:${ax2},${ay2}`]);
    return `dragged (${ax1},${ay1}) -> (${ax2},${ay2})`;
  }

  async function waitForSelector(sel: string, timeoutMs = 10_000): Promise<boolean> {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const res = await evalCode(js.has(sel));
      if (res.ok && res.result === "true") return true;
      await new Promise((resolve) => setTimeout(resolve, 300));
    }
    return false;
  }

  // One-shot flags (--eval/--rect/--screenshot/--drag) make scripted/agent
  // use a supported path instead of piping lines into the REPL's stdin —
  // run exactly one command, print its result, exit with a real code, no
  // REPL banner. No flag given falls through to the REPL below unchanged.
  const [flag, ...flagArgs] = args;
  if (flag?.startsWith("--")) {
    try {
      let output: string;
      switch (flag) {
        case "--eval":
          output = formatResult(await evalCode(flagArgs.join(" ")));
          break;
        case "--rect":
          output = JSON.stringify(await getRect());
          break;
        case "--screenshot": {
          const { rest, selector } = extractSelector(flagArgs);
          if (!rest[0]) throw new Error("usage: chain inspect --screenshot <path> [--selector <sel>]");
          output = await screenshotRegion(rest[0], selector);
          break;
        }
        case "--drag": {
          const { rest, selector } = extractSelector(flagArgs);
          const [x1, y1, x2, y2] = rest.map(Number);
          if ([x1, y1, x2, y2].some(Number.isNaN)) {
            throw new Error("usage: chain inspect --drag <x1> <y1> <x2> <y2> [--selector <sel>]");
          }
          output = await performDrag(x1, y1, x2, y2, selector);
          break;
        }
        default:
          throw new Error(`unknown flag: ${flag}`);
      }
      console.log(output);
      socket.end();
      process.exit(0);
    } catch (e) {
      console.error(`ERROR: ${(e as Error).message}`);
      socket.end();
      process.exit(1);
    }
    return;
  }

  socket.on("close", () => {
    console.log("\n── dev inspector disconnected (did chain dev restart or stop?) — exiting ──");
    process.exit(1);
  });

  console.log("connected. `help` for commands, `quit` to exit.\n");
  const repl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: "inspect> "
  });

  // A plain `repl.on("line", async ...)` fires the next line immediately —
  // it doesn't wait for the previous command's socket round trip. That's
  // invisible when a human types one line at a time, but piped/scripted
  // input (multiple commands at once) would race ahead and `quit` could
  // exit before earlier commands' responses ever print. `for await...of`
  // on the readline interface pulls one line only once the loop body
  // (and everything it awaits) has finished, keeping commands serialized.
  repl.prompt();
  for await (const line of repl) {
    const trimmed = line.trim();
    const spaceIdx = trimmed.indexOf(" ");
    const cmd = spaceIdx === -1 ? trimmed : trimmed.slice(0, spaceIdx);
    const rest = spaceIdx === -1 ? "" : trimmed.slice(spaceIdx + 1);
    try {
      switch (cmd) {
        case "":
          break;
        case "help":
          console.log(
            "commands: eval <js>, click <sel>, click-text <text>, text [sel], wait <sel>, " +
              "type <sel> <text>, rect, screenshot <path> [--selector <sel>], " +
              "drag <x1> <y1> <x2> <y2> [--selector <sel>], quit"
          );
          break;
        case "eval":
          console.log(formatResult(await evalCode(rest)));
          break;
        case "click":
          console.log(formatResult(await evalCode(js.click(rest))));
          break;
        case "click-text":
          console.log(formatResult(await evalCode(js.clickText(rest))));
          break;
        case "text":
          console.log(formatResult(await evalCode(js.text(rest || undefined))));
          break;
        case "wait":
          console.log((await waitForSelector(rest)) ? `found: ${rest}` : `TIMEOUT: ${rest}`);
          break;
        case "rect":
          console.log(JSON.stringify(await getRect()));
          break;
        case "screenshot": {
          const { rest: positional, selector } = extractSelector(splitArgs(rest));
          if (!positional[0]) {
            console.log("usage: screenshot <path> [--selector <sel>]");
            break;
          }
          console.log(await screenshotRegion(positional[0], selector));
          break;
        }
        case "drag": {
          const { rest: positional, selector } = extractSelector(splitArgs(rest));
          const [x1, y1, x2, y2] = positional.map(Number);
          if ([x1, y1, x2, y2].some(Number.isNaN)) {
            console.log("usage: drag <x1> <y1> <x2> <y2> [--selector <sel>]");
            break;
          }
          console.log(await performDrag(x1, y1, x2, y2, selector));
          break;
        }
        case "type": {
          const sp = rest.indexOf(" ");
          if (sp === -1) {
            console.log("usage: type <selector> <text>");
            break;
          }
          console.log(formatResult(await evalCode(js.type(rest.slice(0, sp), rest.slice(sp + 1)))));
          break;
        }
        case "quit":
          socket.end();
          repl.close();
          process.exit(0);
          break;
        default:
          console.log(`unknown command: ${cmd} — try \`help\``);
      }
    } catch (e) {
      console.log(`ERROR: ${(e as Error).message}`);
    }
    repl.prompt();
  }
}
