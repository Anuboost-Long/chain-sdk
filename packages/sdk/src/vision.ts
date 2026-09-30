import { invoke, isTauri } from "@tauri-apps/api/core";

import type {
  RecognizeDocumentOptions,
  RecognizedDocument,
  RecognizeTextOptions,
  RecognizedText,
  VisionApi
} from "./contracts/vision";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = ["INVALID_ARGUMENT", "UNSUPPORTED"];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.vision.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(
  cmd: string,
  args?: Parameters<typeof invoke>[1],
  options?: Parameters<typeof invoke>[2]
): Promise<T> {
  try {
    return await invoke<T>(cmd, args, options);
  } catch (error) {
    const message = typeof error === "string" ? error : "text recognition failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

export const vision: VisionApi = {
  async recognizeText(image: Uint8Array, options?: RecognizeTextOptions): Promise<RecognizedText> {
    requireTauri("recognizeText");
    // The bytes go as the raw IPC body (see templates/lib.rs), the
    // options as a header — a JSON number array would quadruple the size.
    return call<RecognizedText>("vision_recognize_text", image, {
      headers: { "chain-vision-options": JSON.stringify(options ?? {}) }
    });
  },

  async recognizeDocument(
    image: Uint8Array,
    options?: RecognizeDocumentOptions
  ): Promise<RecognizedDocument> {
    requireTauri("recognizeDocument");
    return call<RecognizedDocument>("vision_recognize_document", image, {
      headers: { "chain-vision-options": JSON.stringify(options ?? {}) }
    });
  },

  async languages(): Promise<string[]> {
    requireTauri("languages");
    return call<string[]>("vision_languages");
  }
};
