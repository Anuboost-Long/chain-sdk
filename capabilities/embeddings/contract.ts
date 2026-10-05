/**
 * Structural contract for the Embeddings capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export type EmbeddingPooling = "mean" | "cls";

/** Which prefix applies: e5 models are trained with "query: " and "passage: ". */
export type EmbeddingInput = "query" | "passage";

/**
 * Which files inside an installed model (see desktop.models) are which, by
 * relative name, and how the model is used. From the app's catalog.
 */
export interface EmbeddingModelConfig {
  /** The ONNX file, e.g. "model.onnx". */
  model: string;
  /** The Hugging Face tokenizer, e.g. "tokenizer.json". */
  tokenizer: string;
  /** The installed model holding `tokenizer`, when it's a download of its own. Default: the model's own id. */
  tokenizerModelId?: string;
  /** Default "mean". Ignored for a model that outputs one vector per text itself. */
  pooling?: EmbeddingPooling;
  /** Scale each vector to length 1, so cosine similarity is a dot product. Default true. */
  normalize?: boolean;
  /** Longer texts are cut to this many tokens. Default: the model's own limit (see CONTRACT.md). */
  maxTokens?: number;
  /** Put before every query, e.g. "query: ". Default none. */
  queryPrefix?: string;
  /** Put before every passage, e.g. "passage: ". Default none. */
  passagePrefix?: string;
}

export interface EmbeddingModel {
  modelId: string;
  config: EmbeddingModelConfig;
}

export interface EmbedOptions extends EmbeddingModel {
  as: EmbeddingInput;
  /** Texts run through the model this many at a time. 1–256, default 16. */
  batchSize?: number;
  /** Aborting rejects the call `CANCELLED`, within the batch being run. */
  signal?: AbortSignal;
}

export interface EmbeddedText {
  /** Tokens in the text with its prefix and the model's special tokens — what counts against maxTokens. */
  tokens: number;
  /** Only the first maxTokens tokens were embedded. */
  truncated: boolean;
}

export interface Embeddings {
  /** The length of every vector. */
  dimension: number;
  /** The limit texts were cut to. */
  maxTokens: number;
  /** One per text, in the same order; each has its own buffer. */
  vectors: Float32Array[];
  /** One per text, in the same order. */
  texts: EmbeddedText[];
}

export interface CountTokensOptions extends EmbeddingModel {
  /** Count the prefix this kind of text gets. Omitted: no prefix. */
  as?: EmbeddingInput;
}

export interface TokenCounts {
  /** One per text, counted as EmbeddedText.tokens is. */
  tokens: number[];
  /** The limit embed() cuts texts to. */
  maxTokens: number;
}

export interface EmbeddingsAvailability {
  /** Whether embed() and countTokens() can run here. */
  available: boolean;
  /** Empty when unavailable. */
  pooling: EmbeddingPooling[];
}

export interface EmbeddingsApi {
  availability(): Promise<EmbeddingsAvailability>;
  embed(texts: string[], options: EmbedOptions): Promise<Embeddings>;
  /** With the model's own tokenizer; loads only the tokenizer. */
  countTokens(texts: string[], options: CountTokensOptions): Promise<TokenCounts>;
  /** Frees the loaded model, after the batch in progress, and resolves once it's freed. */
  unload(): Promise<void>;
}
