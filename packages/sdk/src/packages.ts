import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type {
  InstalledPackage,
  PackageInstallProgress,
  PackageManifest,
  PackagesApi
} from "./contracts/packages";
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
      `desktop.packages.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "package operation failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

export const packages: PackagesApi = {
  async install(
    manifest: PackageManifest,
    onProgress?: PackageInstallProgress
  ): Promise<InstalledPackage> {
    requireTauri("install");
    // Progress events carry the package id; one install per id runs at a time.
    const unlisten = onProgress
      ? await listen<{ id: string; received: number; total: number | null }>(
          "chain://packages-progress",
          (event) => {
            if (event.payload.id === manifest.id)
              onProgress(event.payload.received, event.payload.total);
          }
        )
      : undefined;
    try {
      return await call<InstalledPackage>("packages_install", { manifest });
    } finally {
      unlisten?.();
    }
  },

  async resolve(id: string, file: string): Promise<string> {
    requireTauri("resolve");
    return call<string>("packages_resolve", { id, file });
  },

  async cancel(id: string): Promise<void> {
    requireTauri("cancel");
    return call<void>("packages_cancel", { id });
  },

  async list(): Promise<InstalledPackage[]> {
    requireTauri("list");
    return call<InstalledPackage[]>("packages_list");
  },

  async remove(id: string): Promise<void> {
    requireTauri("remove");
    return call<void>("packages_remove", { id });
  }
};
