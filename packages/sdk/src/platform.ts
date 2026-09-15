import { invoke, isTauri } from "@tauri-apps/api/core";

import type { PlatformApi, PlatformInfo } from "../../../capabilities/platform/contract";
import { chainError } from "./errors";

export const platform: PlatformApi = {
  async getInfo(): Promise<PlatformInfo> {
    if (!isTauri()) {
      throw chainError(
        "UNSUPPORTED",
        "desktop.platform.getInfo() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context"
      );
    }
    try {
      return await invoke<PlatformInfo>("get_platform_info");
    } catch (error) {
      if (error === "UNSUPPORTED") {
        throw chainError("UNSUPPORTED", "This platform is not supported yet");
      }
      throw chainError(
        "NATIVE_FAILURE",
        typeof error === "string" ? error : "platform.getInfo() failed"
      );
    }
  }
};
