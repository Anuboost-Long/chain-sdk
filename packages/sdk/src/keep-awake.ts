import { invoke, isTauri } from "./native";
import type { KeepAwakeApi, KeepAwakeOptions, KeepAwakeStatus } from "./contracts/keep-awake";
import { chainError, type ChainErrorCode } from "./errors";

// Native errors arrive as "CODE: message" — see templates/lib.rs.
const CODES: ChainErrorCode[] = ["INVALID_ARGUMENT", "UNSUPPORTED"];

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.keepAwake.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : `keepAwake.${method}() failed`;
    const code = CODES.find((candidate) => message.startsWith(`${candidate}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

export const keepAwake: KeepAwakeApi = {
  start(reason: string, options?: KeepAwakeOptions) {
    return call<void>("start", "keep_awake_start", { reason, display: options?.display });
  },

  stop() {
    return call<void>("stop", "keep_awake_stop");
  },

  status() {
    return call<KeepAwakeStatus | null>("status", "keep_awake_status");
  }
};
