# AudioRecorder capability

## How it works

`desktop.audioRecorder` records natively, outside the webview, so it can
reach the computer's own output. On macOS `swift/ChainRecorder.swift`
builds one private aggregate device per recording: the default input
device for the microphone, a global Core Audio process tap for system
audio, both together for "both" (the aggregate aligns and resamples them
onto one clock). Its IO callback downmixes each source to mono and hands
it to `crates/core/src/audio_recorder.rs`. Its worker thread processes
the microphone (`microphone_processor.rs`, vendored SpeexDSP): for
"both" it cancels the computer's sound echoed into the microphone, with
the tap as reference (on by default). For "microphone" and "both" it can
also suppress noise and control gain (off by default). The thread then
levels and mixes the two (for "both"), meters the level, and streams AAC into a temp
`.m4a` through `m4a.rs`. `stop` finishes the file and the Tauri command
moves it into `desktop.files`; `cancel` deletes it. A page load cancels
a recording left running.

Permissions: `microphone` and `systemAudio` under `chain.permissions`
become `NSMicrophoneUsageDescription` / `NSAudioCaptureUsageDescription`.
Before starting, the bridge asks AVFoundation (microphone) and TCC's
private `TCCAccessPreflight`/`TCCAccessRequest` (system audio, which has
no public API and records silence when refused) and rejects
`PERMISSION_DENIED` on a refusal.

## How to use it

```json
// package.json
"chain": { "permissions": {
  "microphone": "Records lectures you choose to capture.",
  "systemAudio": "Records the lectures and calls your computer plays."
} }
```

```ts
const available = await desktop.audioRecorder.availability();
// { microphone, system, both, echoCancellation, noiseSuppression, autoGainControl }
await desktop.audioRecorder.start({ source: "both", onLevel: (level) => drawWaveform(level) });
// With headphones there's no echo to remove:
await desktop.audioRecorder.start({ source: "both", echoCancellation: false });
// Keep AirPods at full quality: record from the built-in mic instead of theirs.
const { microphone } = await desktop.audioRecorder.start({
  source: "both",
  avoidBluetoothMicrophone: true,
  onMicrophoneChange: ({ microphone, previous }) => notify(`${previous.name} disconnected, now ${microphone.name}`)
});
// Or a specific one: (await desktop.audioRecorder.microphones()) → start({ source, microphone: id })
// A noisy room, a distant voice (gain control raises noise too, so pair them):
await desktop.audioRecorder.start({ source: "microphone", noiseSuppression: true, autoGainControl: true });
await desktop.audioRecorder.pause();
await desktop.audioRecorder.resume();
const { file, mimeType, durationMs } = await desktop.audioRecorder.stop(); // file: a desktop.files reference
```

Errors: `PERMISSION_DENIED` (refused, point to System Settings),
`UNSUPPORTED` (platform/OS, or the permission isn't declared),
`UNAVAILABLE` (already recording / nothing recording).

## Files to check

- `capabilities/audio-recorder/contract.ts`, `CONTRACT.md` — the contract.
- `crates/core/swift/ChainRecorder.swift` — device, tap, permissions, IO callback.
- `crates/core/src/audio_recorder.rs` — state, mixer, level meter, encoder thread.
- `crates/core/src/microphone_processor.rs` — echo cancellation, noise suppression (Speex), and "is someone speaking".
- `crates/core/src/recorder_tracks.rs` — `CHAIN_RECORDER_TRACKS_DIR`: also write each take's separate tracks (raw mic, processed mic, mic in the mix, system) as WAVs, to measure echo and noise on a real recording. See research/PROCESSING.md.
- `crates/core/vendor/speexdsp/`, `crates/core/build.rs` (`build_speexdsp`) — the vendored C and its build.
- `crates/core/src/m4a.rs` — the AAC writer (shared with tts `compile`).
- `packages/cli/templates/lib.rs` (`audio_recorder_*`, `release_abandoned_work`) — Tauri commands.
- `packages/sdk/src/audio-recorder.ts` — SDK wrapper and level events.
- `packages/cli/src/permissions.ts` — the `systemAudio` key.
- `research/MACOS.md`, `research/WINDOWS.md`, `research/PROCESSING.md`.
