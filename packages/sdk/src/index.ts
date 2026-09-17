import { platform } from "./platform";
import { storage } from "./storage";
import { files } from "./files";

/**
 * Public Chain SDK entry point. Applications import `desktop` from
 * "@chain/sdk" and never reach into Tauri, Rust, or native code directly.
 */
export const desktop = {
  platform,
  storage,
  files
};

export type { ChainError, ChainErrorCode } from "./errors";
export type { PlatformInfo, ChainOs, ChainArch } from "../../../capabilities/platform/contract";
export type { StorageApi, Migration, ExecuteResult } from "../../../capabilities/storage/contract";
export type { FilesApi } from "../../../capabilities/files/contract";
