import { listen } from "@tauri-apps/api/event";

import { invoke, isTauri } from "./native";
import type {
  StartTerminalOptions,
  TerminalApi,
  TerminalBacklog,
  TerminalExit,
  TerminalExitHandler,
  TerminalOutput,
  TerminalOutputHandler,
  TerminalSession,
  TerminalUnsubscribe
} from "./contracts/terminal";
import { chainError, type ChainErrorCode } from "./errors";

// Native errors arrive as "CODE: message" — see templates/terminal.rs.
const CODES: ChainErrorCode[] = [
  "INVALID_ARGUMENT",
  "NOT_GRANTED",
  "NOT_FOUND",
  "PERMISSION_DENIED",
  "UNAVAILABLE",
  "TIMEOUT"
];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.terminal.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  requireTauri(method);
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : `terminal.${method}() failed`;
    const code = CODES.find((candidate) => message.startsWith(`${candidate}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

// One shared listener per event for every session and subscriber, set up
// on first use. Native emits each chunk once, however many are listening.
const outputHandlers = new Set<TerminalOutputHandler>();
const exitHandlers = new Set<TerminalExitHandler>();
let listening: Promise<unknown> | null = null;

function ensureListening(): Promise<unknown> {
  listening ??= Promise.all([
    listen<TerminalOutput>("chain://terminal-output", (event) => {
      for (const handler of outputHandlers) handler(event.payload);
    }),
    listen<{ sessionId: string; exit: TerminalExit }>("chain://terminal-exit", (event) => {
      for (const handler of exitHandlers) handler(event.payload.sessionId, event.payload.exit);
    })
  ]);
  return listening;
}

function subscribe<T>(handlers: Set<T>, handler: T): TerminalUnsubscribe {
  if (!isTauri()) return () => {};
  void ensureListening();
  handlers.add(handler);
  return () => {
    handlers.delete(handler);
  };
}

export const terminal: TerminalApi = {
  start(options: StartTerminalOptions) {
    return call<TerminalSession>("start", "terminal_start", { options });
  },

  list() {
    return call<TerminalSession[]>("list", "terminal_list");
  },

  backlog(sessionId: string) {
    return call<TerminalBacklog>("backlog", "terminal_backlog", { sessionId });
  },

  // Listening starts before the backlog is read, and live chunks are held
  // until it arrives: a chunk can reach the page before the backlog's own
  // reply does. Anything at or below the backlog's seq is already in it.
  async attach(sessionId: string, onOutput: TerminalOutputHandler): Promise<TerminalUnsubscribe> {
    requireTauri("attach");
    await ensureListening();
    let held: TerminalOutput[] | null = [];
    let delivered = 0;
    const forward = (chunk: TerminalOutput) => {
      if (chunk.seq <= delivered) return;
      delivered = chunk.seq;
      onOutput(chunk);
    };
    const handler: TerminalOutputHandler = (chunk) => {
      if (chunk.sessionId !== sessionId) return;
      if (held) held.push(chunk);
      else forward(chunk);
    };
    outputHandlers.add(handler);
    try {
      const backlog = await terminal.backlog(sessionId);
      if (backlog.data) onOutput({ sessionId, seq: backlog.seq, data: backlog.data });
      delivered = backlog.seq;
    } catch (error) {
      outputHandlers.delete(handler);
      throw error;
    }
    const pending = held;
    held = null;
    for (const chunk of pending) forward(chunk);
    return () => {
      outputHandlers.delete(handler);
    };
  },

  onOutput(handler: TerminalOutputHandler) {
    return subscribe(outputHandlers, handler);
  },

  onExit(handler: TerminalExitHandler) {
    return subscribe(exitHandlers, handler);
  },

  write(sessionId: string, data: string) {
    return call<void>("write", "terminal_write", { sessionId, data });
  },

  resize(sessionId: string, cols: number, rows: number) {
    return call<void>("resize", "terminal_resize", { sessionId, cols, rows });
  },

  kill(sessionId: string) {
    return call<void>("kill", "terminal_kill", { sessionId });
  },

  remove(sessionId: string) {
    return call<void>("remove", "terminal_remove", { sessionId });
  }
};
