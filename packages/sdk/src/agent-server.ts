import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  AgentServerApi,
  AgentServerHandler,
  AgentServerInfo
} from "./contracts/agent-server";
import { chainError } from "./errors";

const INVALID_ARGUMENT_PREFIX = "INVALID_ARGUMENT: ";
const UNAVAILABLE_PREFIX = "UNAVAILABLE: ";
const PERMISSION_DENIED_PREFIX = "PERMISSION_DENIED: ";

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.agentServer.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

function toChainError(error: unknown, fallback: string) {
  if (typeof error === "string") {
    if (error.startsWith(INVALID_ARGUMENT_PREFIX)) {
      return chainError("INVALID_ARGUMENT", error.slice(INVALID_ARGUMENT_PREFIX.length));
    }
    if (error.startsWith(UNAVAILABLE_PREFIX)) {
      return chainError("UNAVAILABLE", error.slice(UNAVAILABLE_PREFIX.length));
    }
    if (error.startsWith(PERMISSION_DENIED_PREFIX)) {
      return chainError("PERMISSION_DENIED", error.slice(PERMISSION_DENIED_PREFIX.length));
    }
  }
  return chainError("NATIVE_FAILURE", typeof error === "string" ? error : fallback);
}

interface AgentServerRequestEvent {
  id: number;
  method: string;
  path: string;
  headers: Record<string, string>;
  body: string;
}

// Module-level, not per-call: this capability allows only one server (and
// one registered handler) per app run — see CONTRACT.md. Tracks the
// currently-active listener's cleanup so stop() (or a later start()
// replacing it) can tear it down.
let activeUnlisten: (() => void) | null = null;

export const agentServer: AgentServerApi = {
  async start(handler: AgentServerHandler, options?: { port?: number }): Promise<AgentServerInfo> {
    requireTauri("start");

    // Registered *before* the native `start` invoke, not after — a
    // request can only ever arrive once the native side has actually
    // bound the port and returned it from that invoke, so listening
    // first closes the window where an incoming request could arrive
    // before anything is listening for it.
    //
    // Kept out of `activeUnlisten` until the native call actually
    // succeeds: CONTRACT.md requires that calling start() while already
    // running rejects *without* touching the still-running server's
    // handler. Attaching this to `activeUnlisten` eagerly, then tearing
    // the "old" one down unconditionally, would silently orphan a
    // genuinely running server the moment a second start() call failed.
    const candidateUnlisten = await listen<AgentServerRequestEvent>(
      "chain://agent-server-request",
      (event) => {
        const { id, method, path, headers, body } = event.payload;
        handler({ method, path, headers, body }).then(
          (response) => {
            void invoke("__chain_agent_server_respond", {
              id,
              ok: true,
              status: response.status,
              headers: response.headers,
              body: response.body
            });
          },
          (error: unknown) => {
            void invoke("__chain_agent_server_respond", {
              id,
              ok: false,
              error: error instanceof Error ? error.message : String(error)
            });
          }
        );
      }
    );

    let port: number;
    try {
      port = await invoke<number>("agent_server_start", { port: options?.port });
    } catch (error) {
      candidateUnlisten();
      throw toChainError(error, "agentServer.start() failed");
    }

    activeUnlisten?.();
    activeUnlisten = candidateUnlisten;
    return { port };
  },

  async stop(): Promise<void> {
    requireTauri("stop");
    try {
      await invoke<void>("agent_server_stop");
    } finally {
      // Idempotent even if nothing was running — matches agent_server_stop's
      // own idempotency (see CONTRACT.md).
      activeUnlisten?.();
      activeUnlisten = null;
    }
  }
};
