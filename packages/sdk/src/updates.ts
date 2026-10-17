import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type { AvailableUpdate, UpdateProgress, UpdatesApi, UpdatesStatus, UpdatesUnsubscribe } from "./contracts/updates";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = ["UNSUPPORTED", "NOT_FOUND", "UNAVAILABLE", "INTEGRITY_FAILED"];

async function call<T>(method: string, cmd: string): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.updates.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd);
  } catch (error) {
    const message = typeof error === "string" ? error : "updating failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

function subscribe<T>(event: string, listener: (payload: T) => void): UpdatesUnsubscribe {
  if (!isTauri()) return () => {};
  let subscribed = true;
  const registering = listen<T>(event, ({ payload }) => {
    if (subscribed) listener(payload);
  });
  return () => {
    subscribed = false;
    registering.then((unlisten) => unlisten());
  };
}

export const updates: UpdatesApi = {
  async status(): Promise<UpdatesStatus> {
    if (!isTauri()) {
      return { supported: false, reason: "unsupported-platform", message: "not running inside a Chain app", currentVersion: "" };
    }
    return call<UpdatesStatus>("status", "updates_status");
  },
  check: () => call<AvailableUpdate | null>("check", "updates_check"),
  download: () => call<void>("download", "updates_download"),
  installAndRelaunch: () => call<void>("installAndRelaunch", "updates_install"),
  onProgress: (listener) => subscribe<UpdateProgress>("chain://updates-progress", listener)
};
