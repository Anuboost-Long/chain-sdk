import { invoke, isTauri } from "./native";
import type { PageZoomApi } from "./contracts/page-zoom";
import { chainError } from "./errors";

async function call<T>(method: string, cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.pageZoom.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : `pageZoom.${method}() failed`;
    if (message.startsWith("INVALID_ARGUMENT: ")) throw chainError("INVALID_ARGUMENT", message.slice(18));
    throw chainError("NATIVE_FAILURE", message);
  }
}

export const pageZoom: PageZoomApi = {
  async set(factor: number) {
    // JSON can't carry NaN or Infinity; reject them here with the right code.
    if (!Number.isFinite(factor)) throw chainError("INVALID_ARGUMENT", `zoom factor must be a finite number, got ${factor}`);
    return call<void>("set", "page_zoom_set", { factor });
  },

  get() {
    return call<number>("get", "page_zoom_get");
  }
};
