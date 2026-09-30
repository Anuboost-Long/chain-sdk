import { agentServer } from "./agent-server";
import { files } from "./files";
import { http } from "./http";
import { models } from "./models";
import { platform } from "./platform";
import { processRunner } from "./process-runner";
import { speech } from "./speech";
import { storage } from "./storage";
import { tts } from "./tts";
import { vision } from "./vision";

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
  processRunner,
  vision,
  speech,
  models,
  tts
};

export type { ChainError, ChainErrorCode } from "./errors";
export type { PlatformInfo, ChainOs, ChainArch } from "./contracts/platform";
export type { StorageApi, Migration, ExecuteResult } from "./contracts/storage";
export type { FilesApi, PickOptions, PickedFile, SaveOptions, SavedFile } from "./contracts/files";
export type {
  HttpApi,
  HttpResponse,
  HttpRequestConfig,
  HttpMethod,
  HttpData,
  HttpParamValue
} from "./contracts/http";
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
export type {
  VisionApi,
  RecognizeTextOptions,
  RecognizedText,
  RecognizedLine,
  TextBox,
  RecognizeDocumentOptions,
  RecognizedDocument,
  DocumentParagraph,
  RecognizedTable,
  TableCell,
  RecognizedList
} from "./contracts/vision";
export type {
  SpeechApi,
  TranscribeOptions,
  Transcript,
  TranscriptSegment,
  SpeechEngine,
  AsrModelConfig
} from "./contracts/speech";
export type {
  ModelsApi,
  ModelManifest,
  InstalledModel,
  ModelInstallProgress
} from "./contracts/models";
export type {
  TtsApi,
  TtsModelConfig,
  TtsVoice,
  SynthesizeOptions,
  CompiledAudio,
  SegmentTiming
} from "./contracts/tts";
