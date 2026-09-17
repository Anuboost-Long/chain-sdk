import { convertFileSrc, invoke, isTauri } from "@tauri-apps/api/core";

import type { FilesApi } from "../../../capabilities/files/contract";
import { chainError } from "./errors";

const NOT_FOUND_PREFIX = "NOT_FOUND: ";

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.files.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    if (typeof error === "string" && error.startsWith(NOT_FOUND_PREFIX)) {
      throw chainError("NOT_FOUND", error.slice(NOT_FOUND_PREFIX.length));
    }
    throw chainError(
      "NATIVE_FAILURE",
      typeof error === "string" ? error : "files operation failed"
    );
  }
}

export const files: FilesApi = {
  async write(bytes: Uint8Array, options?: { extension?: string }): Promise<string> {
    requireTauri("write");
    // Vec<u8> deserializes from a JSON array of numbers — a raw
    // Uint8Array serializes as {"0":1,"1":2,...} instead, so this
    // conversion is required, not cosmetic (confirmed empirically).
    return call<string>("files_write", { bytes: Array.from(bytes), extension: options?.extension });
  },

  async read(reference: string): Promise<Uint8Array> {
    requireTauri("read");
    const bytes = await call<number[]>("files_read", { reference });
    return new Uint8Array(bytes);
  },

  async url(reference: string): Promise<string> {
    requireTauri("url");
    const path = await call<string>("files_resolve_path", { reference });
    return convertFileSrc(path);
  },

  async delete(reference: string): Promise<void> {
    requireTauri("delete");
    return call<void>("files_delete", { reference });
  }
};
