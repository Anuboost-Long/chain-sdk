export const ansi = {
  reset: "\x1b[0m",
  bold: "\x1b[1m",
  dim: "\x1b[2m",
  cyan: "\x1b[36m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  red: "\x1b[31m",
  gray: "\x1b[90m"
};

export type Color = (code: string, text: string) => string;

export function makeColor(enabled: boolean): Color {
  return (code, text) => (enabled ? `${code}${text}${ansi.reset}` : text);
}

// Only a fixed number of building-status updates per second, so a stream of
// "Compiling X" lines from cargo collapses into occasional progress updates
// instead of one printed line per crate.
const BUILDING_UPDATE_THROTTLE_MS = 400;

export type Status = "starting" | "building" | "ready" | "error" | "stopped";

interface ServiceState {
  status: Status;
  detail: string;
}

interface Tracked extends ServiceState {
  lastPrintedStatus: Status | null;
  lastPrintedAt: number;
}

export interface NativeOutputState {
  frontend: Tracked;
  native: Tracked;
  crateCount: number;
  warnings: number;
  errors: number;
  inDiagnosticBlock: boolean;
}

export function freshState(): NativeOutputState {
  return {
    frontend: { status: "starting", detail: "starting…", lastPrintedStatus: null, lastPrintedAt: 0 },
    native: { status: "starting", detail: "starting…", lastPrintedStatus: null, lastPrintedAt: 0 },
    crateCount: 0,
    warnings: 0,
    errors: 0,
    inDiagnosticBlock: false
  };
}

function glyph(status: Status, color: Color): string {
  switch (status) {
    case "ready":
      return color(ansi.green, "●");
    case "error":
      return color(ansi.red, "✘");
    case "stopped":
      return color(ansi.gray, "○");
    case "building":
      return color(ansi.yellow, "→");
    default:
      return color(ansi.gray, "·");
  }
}

/** Prints a condensed status line for a service, but only on a real status
 * change (starting → building → ready/error) or, while still "building", at
 * most every `BUILDING_UPDATE_THROTTLE_MS` — this is what keeps a stream of
 * "Compiling X v1.2.3" lines from becoming one printed line per crate. */
function maybePrintStatus(label: string, svc: Tracked, color: Color): void {
  const now = Date.now();
  const statusChanged = svc.status !== svc.lastPrintedStatus;
  const throttleElapsed = now - svc.lastPrintedAt > BUILDING_UPDATE_THROTTLE_MS;
  if (!statusChanged && !(svc.status === "building" && throttleElapsed)) return;
  console.log(`  ${label.padEnd(10)} ${glyph(svc.status, color)}  ${svc.detail}`);
  svc.lastPrintedStatus = svc.status;
  svc.lastPrintedAt = now;
}

/** Folds one raw output line from `tauri dev`/`tauri build` (which forwards
 * both Vite and cargo/tauri-cli output on a single stream) into the
 * condensed dashboard state, printing status-line updates as they happen.
 * Diagnostics (warning:/error: blocks, including their indented
 * continuation lines) are always printed in full — condensing never hides
 * a real build failure. */
export function processLine(rawLine: string, state: NativeOutputState, color: Color): void {
  const line = rawLine.replace(/\x1b\[[0-9;]*m/g, "");
  if (!line.trim()) {
    state.inDiagnosticBlock = false;
    return;
  }

  if (state.inDiagnosticBlock) {
    console.log(`  ${color(ansi.dim, line)}`);
    return;
  }

  const compiling = line.match(/^\s*Compiling (\S+)/);
  if (compiling) {
    state.crateCount++;
    state.native.status = "building";
    state.native.detail = `compiling ${compiling[1]} (${state.crateCount} crate${
      state.crateCount === 1 ? "" : "s"
    })`;
    maybePrintStatus("Native", state.native, color);
    return;
  }
  if (/^\s*Finished/.test(line)) {
    state.native.status = "ready";
    state.native.detail = "built";
    maybePrintStatus("Native", state.native, color);
    return;
  }
  if (/^\s*Running `/.test(line)) {
    state.native.status = "ready";
    state.native.detail = "running";
    maybePrintStatus("Native", state.native, color);
    return;
  }
  const viteUrl = line.match(/Local:\s+(\S+)/);
  if (viteUrl) {
    state.frontend.status = "ready";
    state.frontend.detail = viteUrl[1];
    maybePrintStatus("Frontend", state.frontend, color);
    return;
  }
  if (/VITE v[\d.]+/.test(line)) {
    state.frontend.status = "building";
    state.frontend.detail = "starting…";
    maybePrintStatus("Frontend", state.frontend, color);
    return;
  }
  if (/^\s*warning:/i.test(line)) {
    state.warnings++;
    state.inDiagnosticBlock = true;
    console.log(`  ${color(ansi.yellow, line)}`);
    return;
  }
  if (/error(\[|:)/i.test(line)) {
    state.errors++;
    state.inDiagnosticBlock = true;
    if (state.native.status !== "ready") {
      state.native.status = "error";
      maybePrintStatus("Native", state.native, color);
    }
    console.log(`  ${color(ansi.red, line)}`);
    return;
  }
  // Unrecognized lines (Tauri CLI Info/Warn banners, stray tool output, ...)
  // still surface — condensing status never means swallowing output.
  console.log(`  ${color(ansi.dim, line)}`);
}
