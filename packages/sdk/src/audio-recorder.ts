import { invoke, isTauri } from "./native";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AudioRecorderApi,
  FinishedRecording,
  Microphone,
  MicrophoneChange,
  RecordingAvailability,
  RecordingSource,
  RecordingStarted,
  StartRecordingOptions
} from "./contracts/audio-recorder";
import { chainError, type ChainErrorCode } from "./errors";

const PREFIXED_CODES: ChainErrorCode[] = ["UNSUPPORTED", "PERMISSION_DENIED", "UNAVAILABLE"];
const SOURCES: RecordingSource[] = ["microphone", "system", "both"];

function requireTauri(method: string): void {
  if (!isTauri()) {
    throw chainError(
      "UNSUPPORTED",
      `desktop.audioRecorder.${method}() requires running inside a Chain (Tauri) app — it can't work in a plain Node/browser context`
    );
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (error) {
    const message = typeof error === "string" ? error : "audio recording failed";
    const code = PREFIXED_CODES.find((c) => message.startsWith(`${c}: `));
    if (code) throw chainError(code, message.slice(code.length + 2));
    throw chainError("NATIVE_FAILURE", message);
  }
}

// A page load cancels any recording, so ids only need to be unique per page.
let lastId = 0;

// The running recording's level and microphone listeners, removed when it
// stops or is cancelled.
let unlisteners: UnlistenFn[] = [];

function stopListening(): void {
  unlisteners.forEach((unlisten) => unlisten());
  unlisteners = [];
}

/** Listened for before invoking, so nothing is missed, and filtered by id
 * so a stale listener never hears a later recording. */
async function listenFor<T>(event: string, id: string, handle: (payload: T) => void): Promise<UnlistenFn> {
  return listen<{ id: string } & T>(event, ({ payload }) => {
    if (payload.id === id) handle(payload);
  });
}

export const audioRecorder: AudioRecorderApi = {
  async availability(): Promise<RecordingAvailability> {
    if (!isTauri()) {
      return {
        microphone: false,
        system: false,
        both: false,
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
        microphoneChoice: false
      };
    }
    return call<RecordingAvailability>("audio_recorder_availability");
  },

  async microphones(): Promise<Microphone[]> {
    if (!isTauri()) return [];
    return call<Microphone[]>("audio_recorder_microphones");
  },

  async start({
    source,
    microphone,
    avoidBluetoothMicrophone = false,
    echoCancellation = true,
    noiseSuppression = false,
    autoGainControl = false,
    onLevel,
    onMicrophoneChange
  }: StartRecordingOptions): Promise<RecordingStarted> {
    requireTauri("start");
    if (!SOURCES.includes(source)) {
      throw chainError("INVALID_ARGUMENT", `source must be one of ${SOURCES.join(", ")}, got ${String(source)}`);
    }
    const id = String(++lastId);
    const listening = await Promise.all([
      onLevel && listenFor<{ level: number }>("chain://audio-recorder-level", id, ({ level }) => onLevel(level)),
      onMicrophoneChange &&
        listenFor<{ change: MicrophoneChange }>("chain://audio-recorder-microphone", id, ({ change }) =>
          onMicrophoneChange(change)
        )
    ]);
    const unlisten = listening.filter((u): u is UnlistenFn => u !== undefined);
    try {
      const started = await call<RecordingStarted & { microphone: Microphone | null }>("audio_recorder_start", {
        id,
        source,
        microphone: microphone ?? null,
        avoidBluetoothMicrophone,
        echoCancellation,
        noiseSuppression,
        autoGainControl
      });
      stopListening();
      unlisteners = unlisten;
      return { microphone: started.microphone ?? undefined, bluetoothFallback: started.bluetoothFallback };
    } catch (error) {
      unlisten.forEach((u) => u());
      throw error;
    }
  },

  async pause(): Promise<void> {
    requireTauri("pause");
    return call<void>("audio_recorder_pause");
  },

  async resume(): Promise<void> {
    requireTauri("resume");
    return call<void>("audio_recorder_resume");
  },

  async stop(): Promise<FinishedRecording> {
    requireTauri("stop");
    try {
      return await call<FinishedRecording>("audio_recorder_stop");
    } finally {
      stopListening();
    }
  },

  async cancel(): Promise<void> {
    requireTauri("cancel");
    try {
      return await call<void>("audio_recorder_cancel");
    } finally {
      stopListening();
    }
  }
};
