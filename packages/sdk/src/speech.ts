import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type { SpeechApi, Transcript, TranscribeOptions } from "./contracts/speech";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = [
  "NOT_FOUND",
  "INVALID_ARGUMENT",
  "UNSUPPORTED",
  "UNAVAILABLE",
  "PERMISSION_DENIED",
  "CANCELLED"
];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.speech.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "speech recognition failed";
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

export const speech: SpeechApi = {
  async transcribe(
    reference: string,
    options?: TranscribeOptions,
    onProgress?: (fraction: number) => void
  ): Promise<Transcript> {
    requireTauri("transcribe");
    // Caller-supplied id, listened for before invoking, so no progress
    // event is missed and none from another call is misattributed.
    const id = generateId();
    const unlisten = onProgress
      ? await listen<{ id: string; fraction: number }>("chain://speech-progress", (event) => {
          if (event.payload.id === id) onProgress(event.payload.fraction);
        })
      : undefined;
    try {
      return await call<Transcript>("speech_transcribe", {
        id,
        reference,
        locale: options?.locale,
        engine: options?.engine
      });
    } finally {
      unlisten?.();
    }
  },

  async cancel(): Promise<void> {
    requireTauri("cancel");
    return call<void>("speech_cancel");
  },

  async locales(): Promise<string[]> {
    requireTauri("locales");
    return call<string[]>("speech_locales");
  }
};
