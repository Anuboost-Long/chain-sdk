import { agentServer } from "./agent-server";
import { audioRecorder } from "./audio-recorder";
import { browser } from "./browser";
import { embeddings } from "./embeddings";
import { files } from "./files";
import { http } from "./http";
import { models } from "./models";
import { pdf } from "./pdf";
import { platform } from "./platform";
import { processRunner } from "./process-runner";
import { share } from "./share";
import { speech } from "./speech";
import { storage } from "./storage";
import { tts } from "./tts";
import { vision } from "./vision";
import { appWindow } from "./window";

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
  tts,
  audioRecorder,
  browser,
  embeddings,
  window: appWindow,
  pdf,
  share
};

export { sql } from "./storage-table";

export type { ChainError, ChainErrorCode } from "./errors";
export type { PlatformInfo, ChainOs, ChainArch } from "./contracts/platform";
export type {
  StorageApi,
  StorageScope,
  StorageTable,
  TableQuery,
  Migration,
  ExecuteResult,
  SqlFragment,
  Column,
  Filter,
  Values,
  OrderBy,
  Target
} from "./contracts/storage";
export type { FilesApi, PickOptions, PickedFile, SaveOptions, SavedFile } from "./contracts/files";
export type {
  HttpApi,
  HttpResponse,
  HttpError,
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
export type {
  AudioRecorderApi,
  RecordingSource,
  RecordingAvailability,
  StartRecordingOptions,
  FinishedRecording,
  Microphone,
  MicrophoneTransport,
  MicrophoneChange,
  RecordingStarted
} from "./contracts/audio-recorder";
export type {
  BrowserApi,
  BrowserAvailability,
  BrowserButton,
  BrowserButtonPress,
  BrowserClosed,
  BrowserFetchError,
  BrowserFetchOptions,
  BrowserFetchResponse,
  BrowserFrame,
  BrowserOpenOptions,
  BrowserPage,
  BrowserPageContent,
  BrowserSessionOptions,
  BrowserUnsubscribe
} from "./contracts/browser";
export type {
  EmbeddingsApi,
  EmbeddingsAvailability,
  EmbeddingModel,
  EmbeddingModelConfig,
  EmbeddingPooling,
  EmbeddingInput,
  EmbedOptions,
  Embeddings,
  EmbeddedText,
  CountTokensOptions,
  TokenCounts
} from "./contracts/embeddings";
export type {
  WindowApi,
  WindowAvailability,
  WindowOptions,
  WindowStartupOptions,
  ShowWhen,
  WindowButtonsOptions,
  WindowButtonsPosition,
  WindowAppearance,
  WindowRect,
  WindowUnsubscribe,
  ResolvedWindowOptions,
  TitleBarInsets,
  TitleBarSize,
  TitleBarStyle
} from "./contracts/window";
export type {
  PdfApi,
  PdfAvailability,
  RenderPdfOptions,
  RenderedPdf,
  PaperSize,
  PaperDimensions,
  PageOrientation,
  PageMargins,
  PageMarginText
} from "./contracts/pdf";
export type {
  ShareApi,
  ShareAvailability,
  ShareOptions,
  ShareResult,
  ShareAnchor,
  SharedFile
} from "./contracts/share";
