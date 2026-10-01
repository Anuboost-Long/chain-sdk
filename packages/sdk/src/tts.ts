import { invoke, isTauri } from "./native";
import { listen } from "@tauri-apps/api/event";

import type {
  CompiledAudio,
  SynthesizeOptions,
  TtsApi,
  TtsModelConfig,
  TtsVoice
} from "./contracts/tts";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = [
  "INVALID_ARGUMENT",
  "NOT_FOUND",
  "UNSUPPORTED",
  "UNAVAILABLE",
  "CANCELLED"
];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.tts.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "speech synthesis failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

function generateId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export const tts: TtsApi = {
  async voices(modelId: string, config: TtsModelConfig): Promise<TtsVoice[]> {
    requireTauri("voices");
    return call<TtsVoice[]>("tts_voices", { modelId, config });
  },

  async synthesize(text: string, options: SynthesizeOptions): Promise<string> {
    requireTauri("synthesize");
    return call<string>("tts_synthesize", {
      text,
      modelId: options.modelId,
      config: options.config,
      voice: options.voice,
      speed: options.speed
    });
  },

  async compile(
    segments: string[],
    options: SynthesizeOptions,
    onProgress?: (fraction: number) => void
  ): Promise<CompiledAudio> {
    requireTauri("compile");
    // Caller-supplied id, listened for before invoking, so no progress
    // event is missed and none from another call is misattributed.
    const id = generateId();
    const unlisten = onProgress
      ? await listen<{ id: string; fraction: number }>("chain://tts-progress", (event) => {
          if (event.payload.id === id) onProgress(event.payload.fraction);
        })
      : undefined;
    try {
      return await call<CompiledAudio>("tts_compile", {
        id,
        segments,
        modelId: options.modelId,
        config: options.config,
        voice: options.voice,
        speed: options.speed
      });
    } finally {
      unlisten?.();
    }
  },

  async cancel(): Promise<void> {
    requireTauri("cancel");
    return call<void>("tts_cancel");
  }
};
