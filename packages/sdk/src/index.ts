import { platform } from "./platform";
import { storage } from "./storage";
import { files } from "./files";
import { http } from "./http";
import { agentServer } from "./agent-server";
import { processRunner } from "./process-runner";

/**
 * Public Chain SDK entry point. Applications import `desktop` from
 * "@chain/sdk" and never reach into Tauri, Rust, or native code directly.
 */
export const desktop = {
  platform,
  storage,
  files,
  http,
  agentServer,
  processRunner
};

export type { ChainError, ChainErrorCode } from "./errors";
export type { PlatformInfo, ChainOs, ChainArch } from "./contracts/platform";
export type { StorageApi, Migration, ExecuteResult } from "./contracts/storage";
export type { FilesApi, PickOptions, PickedFile, SaveOptions, SavedFile } from "./contracts/files";
export type { HttpApi, HttpResponse } from "./contracts/http";
export type {
  AgentServerApi,
  AgentServerRequest,
  AgentServerResponse,
  AgentServerHandler,
  AgentServerInfo
} from "./contracts/agent-server";
export type {
  ProcessRunnerApi,
  ProcessArg,
  ProcessHandle,
  ProcessOutputChunk,
  ProcessOutputHandler,
  ProcessRunOptions,
  ProcessExit
} from "./contracts/process-runner";
