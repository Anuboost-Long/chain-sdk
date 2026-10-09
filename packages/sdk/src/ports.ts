import { invoke, isTauri } from "./native";
import type { PortsApi } from "./contracts/ports";
import { chainError } from "./errors";

export const ports: PortsApi = {
  async isFree(port: number): Promise<boolean> {
    if (!isTauri()) {
      throw chainError(
        "UNSUPPORTED",
        "desktop.ports.isFree() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context"
      );
    }
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      throw chainError("INVALID_ARGUMENT", `port must be an integer between 1 and 65535, got ${port}`);
    }
    try {
      return await invoke<boolean>("ports_is_free", { port });
    } catch (error) {
      const message = typeof error === "string" ? error : "ports.isFree() failed";
      if (message.startsWith("INVALID_ARGUMENT: ")) throw chainError("INVALID_ARGUMENT", message.slice(18));
      throw chainError("NATIVE_FAILURE", message);
    }
  }
};
