import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  ProcessArg,
  ProcessExit,
  ProcessHandle,
  ProcessOutputHandler,
  ProcessRunOptions,
  ProcessRunnerApi
} from "./contracts/process-runner";
import { chainError } from "./errors";

const INVALID_ARGUMENT_PREFIX = "INVALID_ARGUMENT: ";
const NOT_FOUND_PREFIX = "NOT_FOUND: ";
const PERMISSION_DENIED_PREFIX = "PERMISSION_DENIED: ";

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.processRunner.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

function toChainError(error: unknown, fallback: string) {
  if (typeof error === "string") {
    if (error.startsWith(INVALID_ARGUMENT_PREFIX)) {
      return chainError("INVALID_ARGUMENT", error.slice(INVALID_ARGUMENT_PREFIX.length));
    }
    if (error.startsWith(NOT_FOUND_PREFIX)) {
      return chainError("NOT_FOUND", error.slice(NOT_FOUND_PREFIX.length));
    }
    if (error.startsWith(PERMISSION_DENIED_PREFIX)) {
      return chainError("PERMISSION_DENIED", error.slice(PERMISSION_DENIED_PREFIX.length));
    }
  }
  return chainError("NATIVE_FAILURE", typeof error === "string" ? error : fallback);
}

interface ProcessOutputEvent {
  id: string;
  stream: "stdout" | "stderr";
  data: string;
}

interface ProcessExitEvent {
  id: string;
  code: number | null;
  killed: boolean;
}

interface PendingProcess {
  onOutput: ProcessOutputHandler;
  resolveExit: (exit: ProcessExit) => void;
}

// Module-level: one shared pair of listeners multiplexes every running
// process by id, rather than each run() call registering (and later
// tearing down) its own. Set up lazily, once, the first time run() is
// ever called.
const pending = new Map<string, PendingProcess>();
let listenersReady: Promise<void> | null = null;

async function ensureListeners(): Promise<void> {
  if (!listenersReady) {
    listenersReady = Promise.all([
      listen<ProcessOutputEvent>("chain://process-output", (event) => {
        pending.get(event.payload.id)?.onOutput({ stream: event.payload.stream, data: event.payload.data });
      }),
      listen<ProcessExitEvent>("chain://process-exit", (event) => {
        const entry = pending.get(event.payload.id);
        if (!entry) return;
        pending.delete(event.payload.id);
        entry.resolveExit({ code: event.payload.code, killed: event.payload.killed });
      })
    ]).then(() => undefined);
  }
  return listenersReady;
}

function generateProcessId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export const processRunner: ProcessRunnerApi = {
  async run(
    command: string,
    args: ProcessArg[],
    onOutput: ProcessOutputHandler,
    options?: ProcessRunOptions
  ): Promise<ProcessHandle> {
    requireTauri("run");

    // Listener attached, and this id registered in `pending`, *before*
    // native is ever asked to spawn anything — a fast process (spawn,
    // print, exit) can finish within a single IPC round trip, so if we
    // invoked first and only started listening after, its output could
    // already have fired (and been lost) before anyone was listening.
    // Generating the id client-side, not letting native hand one back,
    // is what makes registering-before-invoking possible at all. See
    // packages/cli/templates/lib.rs's matching comment on
    // process_runner_run.
    await ensureListeners();
    const id = generateProcessId();
    let exitResolve!: (exit: ProcessExit) => void;
    const exited = new Promise<ProcessExit>((resolve) => {
      exitResolve = resolve;
    });
    pending.set(id, { onOutput, resolveExit: exitResolve });

    try {
      await invoke<void>("process_runner_run", { id, command, args, stdin: options?.stdin });
    } catch (error) {
      pending.delete(id);
      throw toChainError(error, "processRunner.run() failed");
    }

    return {
      id,
      async kill(): Promise<void> {
        requireTauri("kill");
        try {
          await invoke<void>("process_runner_kill", { id });
        } catch (error) {
          throw toChainError(error, "processRunner kill() failed");
        }
      },
      exited
    };
  }
};
