# Speech Capability — Agent Memory

Scope: `desktop.speech.transcribe(reference, {locale}, onProgress)`,
`cancel()`, `locales()`. Requested by mneme
(`docs/chain-sdk-requests/19-speech-transcription.md` in the mneme repo)
for Phase 17.

## What's already decided

- **SpeechAnalyzer through a Swift bridge on macOS 26+.** It's Swift-only
  (no ObjC surface), so this is chain-core's first Swift code. It's worth
  it: unlike `SFSpeechRecognizer` it works with Siri & Dictation off, needs
  no permission prompt, downloads models itself, and handles long-form
  audio (60 min in 51 s). `SFSpeechRecognizer` on-device stays as the
  pre-26 fallback; it failed here with "Siri and Dictation are disabled".
- **Never server recognition**: the legacy path always sets
  `requiresOnDeviceRecognition`; SpeechAnalyzer is on-device by design.
- **C ABI + JSON across the Swift boundary**, callbacks with a leaked
  `Sender` as context (freed by Rust after the finish callback). Never
  Swift or Foundation objects.
- **Linking**: `build.rs` puts `<SDK>/usr/lib/swift` before the toolchain
  runtime path. Even so, an app binary targeting < macOS 12 (Rust's
  default 11.0; `tauri dev` never sets a target) links
  `libswift_Concurrency` by `@rpath`. Hence the app template `build.rs`
  (rpath `/usr/lib/swift`) and `minimumSystemVersion: 12.0`. A library's
  build script can't add rpaths for its dependents.
- **Segments are grouped in Rust** from token timings, so both backends
  segment the same way.
- One run at a time (`RunningGuard`); `cancel()` sets a flag so a cancel
  that races the task's creation still lands.

## Status

- [x] macOS 26.6 (Apple Silicon), SpeechAnalyzer path: probe `.app` and end to
      end through `desktop.speech` in `apps/playground` — short lecture exact
      with 3 segments and 33 progress callbacks; 59.9 min lecture in 51 s,
      746 segments, continuous to 3591.5 s; second call `UNAVAILABLE`;
      `cancel()` → `CANCELLED`; missing reference `NOT_FOUND`; `tlh-XX`
      `UNSUPPORTED`; a real `MediaRecorder` AAC recording from request 18
      transcribed.
- [~] Legacy `SFSpeechRecognizer` path: authorization prompt and the
  "Dictation disabled" error observed; a successful transcription on
  pre-26 macOS isn't verified (no such machine here).
- [x] Video files and Opus (request 39), macOS 26.6, 6 Oct 2026. In
      `cargo test`, with CHAIN_TEST_SYSTEM_SPEECH and
      CHAIN_TEST_SPEECH_MODELS=<app models dir> set, the same sentence was
      transcribed word for word from MP4/MOV/M4V AAC, WebM/MKV Opus, a
      duration-less browser WebM and Ogg .opus/.ogg, on the system engine
      and on whisper-base. A silent MP4 is `NOT_FOUND` on both. A 20-min
      WebM took 11.5 s, with paced progress and timestamps continuous to
      1183 s. Cancelling the decoded path mid-run is clean. The same was
      run end to end through `desktop.speech` in the playground.
- [~] Legacy path with decoded sound (`SFSpeechAudioBufferRecognitionRequest`):
      compiled, not run (no pre-26 Mac).
- [ ] Model download from scratch (en-US was already installed here).
- [ ] Windows: deliberately `UNSUPPORTED` — WinRT speech can't take a file;
      options in research/WINDOWS.md.
- [ ] Contract tests under `capabilities/speech/tests/`.
