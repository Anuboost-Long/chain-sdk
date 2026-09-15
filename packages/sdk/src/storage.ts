import { invoke, isTauri } from "@tauri-apps/api/core";

import type { StorageApi, Migration, ExecuteResult } from "../../../capabilities/storage/contract";
import { chainError } from "./errors";

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      "desktop.storage requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context"
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    throw chainError(
      "NATIVE_FAILURE",
      typeof error === "string" ? error : "storage operation failed"
    );
  }
}

export const storage: StorageApi = {
  migrate(migrations: Migration[]): Promise<void> {
    return call("storage_migrate", { migrations });
  },
  query<T = unknown>(sql: string, params: unknown[] = []): Promise<T[]> {
    return call("storage_query", { sql, params });
  },
  execute(sql: string, params: unknown[] = []): Promise<ExecuteResult> {
    return call("storage_execute", { sql, params });
  }
};
