import { invoke, isTauri } from "./native";

import type {
  CountTokensOptions,
  EmbedOptions,
  Embeddings,
  EmbeddingsApi,
  EmbeddingsAvailability,
  TokenCounts
} from "./contracts/embeddings";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = [
  "INVALID_ARGUMENT",
  "NOT_FOUND",
  "UNSUPPORTED",
  "TOO_LARGE",
  "CANCELLED"
];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.embeddings.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "embedding failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

function cancelled(): never {
  throw chainError("CANCELLED", "the embedding was cancelled");
}

// embeddings_embed replies with raw bytes (Embedded::to_bytes in
// crates/core/src/embeddings): little-endian u32 dimension, maxTokens and
// count; per text u32 tokens and truncated (0/1); then the f32 vectors.
function unframe(buffer: ArrayBuffer): Embeddings {
  const view = new DataView(buffer);
  const dimension = view.getUint32(0, true);
  const maxTokens = view.getUint32(4, true);
  const count = view.getUint32(8, true);
  const texts = Array.from({ length: count }, (_, i) => ({
    tokens: view.getUint32(12 + i * 8, true),
    truncated: view.getUint32(16 + i * 8, true) === 1
  }));
  const start = 12 + count * 8;
  const size = dimension * 4;
  const vectors = Array.from(
    { length: count },
    (_, i) => new Float32Array(buffer.slice(start + i * size, start + (i + 1) * size))
  );
  return { dimension, maxTokens, vectors, texts };
}

export const embeddings: EmbeddingsApi = {
  async availability(): Promise<EmbeddingsAvailability> {
    if (!isTauri()) return { available: false, pooling: [] };
    return call<EmbeddingsAvailability>("embeddings_availability");
  },

  async embed(texts: string[], options: EmbedOptions): Promise<Embeddings> {
    requireTauri("embed");
    const { signal } = options;
    if (signal?.aborted) cancelled();
    const id = crypto.randomUUID();
    const onAbort = () => void invoke("embeddings_cancel", { id });
    signal?.addEventListener("abort", onAbort, { once: true });
    try {
      const buffer = await call<ArrayBuffer>("embeddings_embed", {
        id,
        texts,
        modelId: options.modelId,
        config: options.config,
        input: options.as,
        batchSize: options.batchSize
      });
      return unframe(buffer);
    } finally {
      signal?.removeEventListener("abort", onAbort);
    }
  },

  async countTokens(texts: string[], options: CountTokensOptions): Promise<TokenCounts> {
    requireTauri("countTokens");
    return call<TokenCounts>("embeddings_count_tokens", {
      texts,
      modelId: options.modelId,
      config: options.config,
      input: options.as
    });
  },

  async unload(): Promise<void> {
    requireTauri("unload");
    return call<void>("embeddings_unload");
  }
};
