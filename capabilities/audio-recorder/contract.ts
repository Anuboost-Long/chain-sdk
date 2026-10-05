/**
 * Structural contract for the AudioRecorder capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

/** "system" is what the computer itself plays; "both" mixes it with the microphone into one track. */
export type RecordingSource = "microphone" | "system" | "both";

/** How a microphone is connected. A Bluetooth headset mic records at call quality. */
export type MicrophoneTransport = "built-in" | "bluetooth" | "usb" | "other";

export interface Microphone {
  /** Stable while the device exists, across launches; pass it to start({ microphone }). */
  id: string;
  name: string;
  transport: MicrophoneTransport;
  /** Whether it's the system's default input. */
  isDefault: boolean;
  /** In Hz, as the device runs now. */
  sampleRate: number;
}

/** Which sources can record here — OS version, platform, and the app's declared permissions. */
export interface RecordingAvailability extends Record<RecordingSource, boolean> {
  /** Whether "both" can remove the computer's sound echoed into the microphone. */
  echoCancellation: boolean;
  /** Whether the microphone's steady background noise can be reduced. */
  noiseSuppression: boolean;
  /** Whether the microphone's level can be evened out. */
  autoGainControl: boolean;
  /** Whether microphones() lists inputs and start() can record from a chosen one. */
  microphoneChoice: boolean;
}

export interface StartRecordingOptions {
  source: RecordingSource;
  /** "microphone" and "both": a Microphone id to record from. Default: the system's default input. Never changes the default. */
  microphone?: string;
  /** Without `microphone`: when the default input is Bluetooth, record from another one (the built-in first), so a headset keeps its full-quality sound. Default false. */
  avoidBluetoothMicrophone?: boolean;
  /** When the recording moves to another microphone because its own disappeared. */
  onMicrophoneChange?: (change: MicrophoneChange) => void;
  /** "both" only: remove the computer's sound the microphone picks up from the speakers. Default true; ignored for one source. */
  echoCancellation?: boolean;
  /** Microphone, in "microphone" and "both": reduce steady background noise (fans, hum, hiss). Default false; ignored for "system". */
  noiseSuppression?: boolean;
  /** "microphone": bring a quiet or distant voice up to an even level. Default false. "both" always evens out the microphone; ignored for "system". */
  autoGainControl?: boolean;
  /** About ten times a second while recording (not while paused): 0–1, dB-scaled peak of what's recorded. */
  onLevel?: (level: number) => void;
}

export interface RecordingStarted {
  /** The microphone recording; absent for "system". */
  microphone?: Microphone;
  /** It's a Bluetooth mic only because no other input exists (avoidBluetoothMicrophone). */
  bluetoothFallback: boolean;
}

export interface MicrophoneChange {
  /** The microphone now recording. */
  microphone: Microphone;
  /** The one that disappeared. */
  previous: Microphone;
  bluetoothFallback: boolean;
}

export interface FinishedRecording {
  /** A desktop.files reference; the app deletes it with files.delete(). */
  file: string;
  /** "audio/mp4" (AAC). */
  mimeType: string;
  /** Recorded time, paused spans left out. */
  durationMs: number;
}

export interface AudioRecorderApi {
  availability(): Promise<RecordingAvailability>;
  /** The connected inputs. Empty where microphoneChoice is false. */
  microphones(): Promise<Microphone[]>;
  /** Resolves once recording has begun, after any permission prompt, with the microphone it uses. */
  start(options: StartRecordingOptions): Promise<RecordingStarted>;
  pause(): Promise<void>;
  resume(): Promise<void>;
  stop(): Promise<FinishedRecording>;
  /** Stops and discards the take and its file. No-op when nothing is recording. */
  cancel(): Promise<void>;
}
