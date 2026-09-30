/**
 * Structural contract for the Tts capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/**
 * Which files and folders inside an installed voice model (see
 * desktop.models) are which, by relative name, and the model's family.
 */
export type TtsModelConfig =
  | {
      type: "kokoro";
      model: string;
      voices: string;
      tokens: string;
      /** The espeak-ng-data folder. */
      dataDir?: string;
      /** The jieba dictionary folder (Chinese). */
      dictDir?: string;
      lexicon?: string[];
      lang?: string;
      /** Speaker names in id order; the model files don't carry them. */
      speakers?: string[];
    }
  | {
      /** Piper and other VITS voices. */
      type: "vits";
      model: string;
      tokens: string;
      dataDir?: string;
      dictDir?: string;
      lexicon?: string[];
      speakers?: string[];
    }
  | {
      type: "kitten";
      model: string;
      voices: string;
      tokens: string;
      dataDir?: string;
      speakers?: string[];
    };

export interface TtsVoice {
  /** Pass as `voice` to synthesize. */
  id: number;
  name: string;
  /** BCP-47, when known. */
  language?: string;
}

export interface SynthesizeOptions {
  modelId: string;
  config: TtsModelConfig;
  /** A TtsVoice id; defaults to 0. */
  voice?: number;
  /** 0.25–4; defaults to 1. */
  speed?: number;
}

/** Where one input segment sits in a compiled recording, in seconds. */
export interface SegmentTiming {
  start: number;
  end: number;
}

export interface CompiledAudio {
  /** A desktop.files reference to an AAC `.m4a`; the app deletes it with files.delete(). */
  file: string;
  /** Seconds. */
  duration: number;
  /** One per input segment, in the same order; back to back, no gaps. */
  segments: SegmentTiming[];
}

export interface TtsApi {
  voices(modelId: string, config: TtsModelConfig): Promise<TtsVoice[]>;
  /** Resolves a desktop.files reference to a 16-bit mono WAV. */
  synthesize(text: string, options: SynthesizeOptions): Promise<string>;
  /**
   * Speaks `segments` (e.g. a page's sentences) in order into one AAC
   * file, with each segment's timing for highlighting along the audio.
   * `onProgress` gets 0–1, by characters spoken.
   */
  compile(
    segments: string[],
    options: SynthesizeOptions,
    onProgress?: (fraction: number) => void
  ): Promise<CompiledAudio>;
  /** Stops the running compile, which rejects `CANCELLED`. No-op when idle. */
  cancel(): Promise<void>;
}
