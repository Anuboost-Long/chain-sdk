import { invoke, isTauri } from "./native";

import type { ShareApi, ShareAvailability, ShareOptions, ShareResult, SharedFile } from "./contracts/share";
import { chainError, type ChainErrorCode } from "./errors";

const NOTHING_AVAILABLE: ShareAvailability = {
  available: false,
  anchor: false,
  text: false,
  title: false,
  serviceName: false
};

// Native errors arrive as "CODE: message" — see templates/lib.rs.
const CODES: ChainErrorCode[] = ["INVALID_ARGUMENT", "NOT_FOUND", "UNAVAILABLE", "UNSUPPORTED"];

export const share: ShareApi = {
  async availability(): Promise<ShareAvailability> {
    if (!isTauri()) return NOTHING_AVAILABLE;
    return invoke<ShareAvailability>("share_availability");
  },

  async show(files: SharedFile[], options?: ShareOptions): Promise<ShareResult> {
    if (!isTauri()) {
      throw chainError(
        "UNSUPPORTED",
        "desktop.share.show() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context"
      );
    }
    try {
      return await invoke<ShareResult>("share_show", { files, options });
    } catch (error) {
      const message = typeof error === "string" ? error : "the share menu failed";
      const code = CODES.find((candidate) => message.startsWith(`${candidate}: `));
      if (code) throw chainError(code, message.slice(code.length + 2));
      throw chainError("NATIVE_FAILURE", message);
    }
  }
};
