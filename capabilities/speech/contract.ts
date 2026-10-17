/**
 * Structural contract for the Speech capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/**
 * Which files inside an installed model are which, by relative name, and
 * the model's family. Mirrors sherpa-onnx's offline model types.
 */
export type AsrModelConfig =
  | { type: "whisper"; encoder: string; decoder: string; tokens: string; language?: string }
  | { type: "sense-voice"; model: string; tokens: string; language?: string; useItn?: boolean }
  | {
      type: "moonshine";
      encoder: string;
      tokens: string;
      /** v1 packs. */
      preprocessor?: string;
      uncachedDecoder?: string;
      cachedDecoder?: string;
      /** v2 packs. */
      mergedDecoder?: string;
    }
  | { type: "paraformer"; model: string; tokens: string }
  | { type: "transducer"; encoder: string; decoder: string; joiner: string; tokens: string }
  | { type: "nemo-ctc"; model: string; tokens: string };

/** Transcribe with an installed model (see desktop.models) instead of the OS engine. */
export interface SpeechEngine {
  modelId: string;
  config: AsrModelConfig;
  /** Silero VAD, installed as its own model; `model` is its file name. */
  vad: { modelId: string; model: string };
}

export interface TranscribeOptions {
  /** BCP-47, e.g. "en-US". Omitted: the system locale. */
  locale?: string;
  /** Omitted: the OS's own recognizer. */
  engine?: SpeechEngine;
}

export interface TranscriptSegment {
  startMs: number;
  endMs: number;
  text: string;
}

export interface Transcript {
  text: string;
  segments: TranscriptSegment[];
  /** The BCP-47 locale actually used. */
  locale: string;
}

export interface SpeechApi {
  /**
   * Transcribes a `desktop.files` reference on-device: an audio file, or a
   * video file's first sound track. One at a time. A file with no sound
   * track rejects `NOT_FOUND`.
   */
  transcribe(
    reference: string,
    options?: TranscribeOptions,
    onProgress?: (fraction: number) => void
  ): Promise<Transcript>;
  /** Stops the running transcription, which rejects `CANCELLED`. No-op when idle. */
  cancel(): Promise<void>;
  /** BCP-47 locales `transcribe` accepts on this machine. */
  locales(): Promise<string[]>;
}
