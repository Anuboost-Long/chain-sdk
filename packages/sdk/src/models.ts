import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type {
  InstalledModel,
  ModelInstallProgress,
  ModelManifest,
  ModelsApi
} from "./contracts/models";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = [
  "INVALID_ARGUMENT",
  "NOT_FOUND",
  "UNAVAILABLE",
  "INTEGRITY_FAILED",
  "CANCELLED"
];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.models.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "model operation failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

export const models: ModelsApi = {
  async install(
    manifest: ModelManifest,
    onProgress?: ModelInstallProgress
  ): Promise<InstalledModel> {
    requireTauri("install");
    // Progress events carry the model id; one install per id runs at a time.
    const unlisten = onProgress
      ? await listen<{ id: string; received: number; total: number | null }>(
          "chain://models-progress",
          (event) => {
            if (event.payload.id === manifest.id)
              onProgress(event.payload.received, event.payload.total);
          }
        )
      : undefined;
    try {
      return await call<InstalledModel>("models_install", { manifest });
    } finally {
      unlisten?.();
    }
  },

  async cancel(id: string): Promise<void> {
    requireTauri("cancel");
    return call<void>("models_cancel", { id });
  },

  async list(): Promise<InstalledModel[]> {
    requireTauri("list");
    return call<InstalledModel[]>("models_list");
  },

  async remove(id: string): Promise<void> {
    requireTauri("remove");
    return call<void>("models_remove", { id });
  }
};
