import { invoke, isTauri } from "@tauri-apps/api/core";

import type { HttpApi, HttpResponse } from "./contracts/http";
import { chainError } from "./errors";

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.http.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

const INVALID_ARGUMENT_PREFIX = "INVALID_ARGUMENT: ";
const UNAVAILABLE_PREFIX = "UNAVAILABLE: ";

export const http: HttpApi = {
  async get(url: string): Promise<HttpResponse> {
    requireTauri("get");
    try {
      return await invoke<HttpResponse>("http_get", { url });
    } catch (error) {
      if (typeof error === "string" && error.startsWith(INVALID_ARGUMENT_PREFIX)) {
        throw chainError("INVALID_ARGUMENT", error.slice(INVALID_ARGUMENT_PREFIX.length));
      }
      if (typeof error === "string" && error.startsWith(UNAVAILABLE_PREFIX)) {
        throw chainError("UNAVAILABLE", error.slice(UNAVAILABLE_PREFIX.length));
      }
      throw chainError("NATIVE_FAILURE", typeof error === "string" ? error : "http request failed");
    }
  }
};
