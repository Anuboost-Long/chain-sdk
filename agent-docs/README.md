# Agent Docs

One folder per shipped feature of this repo (`chain-sdk`), written for
whoever — human or AI agent — needs to use, extend, or debug it without
re-deriving how it works from scratch. Split into two groups:

- **`framework/`** — the fixed pieces of Chain itself: the CLI, the SDK,
  Chain Core, visual identity. There's one of each.
- **`capabilities/`** — the pluggable modules (`platform`, `storage`, ...),
  mirroring `capabilities/<name>/` by name. Each capability folder here
  also holds its `AGENTS.md` (build-time memory) and `CONTRACT.md`
  (semantic contract) — a different _kind_ of doc from the `README.md`
  below (see root `AGENTS.md`), kept alongside it so `capabilities/<name>/`
  itself stays code-only (`contract.ts`, `component.json`, `tests/`).

## Convention

Every feature gets a `README.md` (under `framework/<name>/` or
`capabilities/<name>/`, whichever it is) with exactly these three
sections:

1. **How it works** — the mechanism, in plain terms. Not a copy of the
   code; the _why_ and the _shape_.
2. **How to use it** — the actual commands / imports / API calls a
   developer runs.
3. **Files to check** — where to look first when something breaks or
   needs extending, one line each on what that file is responsible for.

**When you add or materially change a feature, add or update its
`README.md` in the same change, and add it to the index below.** This is
a standing instruction, not a one-time task — it repeats for every
feature added to this SDK.

## Index

| Feature                  | Folder                                                                  | What it is                                                                                               |
| ------------------------ | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Chain CLI                | [`framework/command/`](framework/command/README.md)                     | `chain init`/`dev`/`build`/`update`/`doctor`, `--help`, `--version`                                      |
| Chain SDK                | [`framework/sdk/`](framework/sdk/README.md)                             | `@chain/sdk` — the public TS API apps import                                                             |
| Visual identity          | [`framework/brand/`](framework/brand/README.md)                         | Chain logo assets and usage                                                                              |
| Documentation site       | [`framework/site/`](framework/site/README.md)                           | `site/` — the public Next.js docs site for app developers                                                |
| Chain Core               | [`framework/core/`](framework/core/README.md)                           | `crates/core` — the Rust coordination crate                                                              |
| Platform capability      | [`capabilities/platform/`](capabilities/platform/README.md)             | `desktop.platform.getInfo()` end-to-end slice                                                            |
| Storage capability       | [`capabilities/storage/`](capabilities/storage/README.md)               | `desktop.storage` — SQLite: migrations, typed table(), transactions                                      |
| Files capability         | [`capabilities/files/`](capabilities/files/README.md)                   | `desktop.files` — managed local blob storage (write/read/url/delete)                                     |
| Http capability          | [`capabilities/http/`](capabilities/http/README.md)                     | `desktop.http` — native-side HTTP requests, axios-style (method, headers, params, body)                  |
| AgentServer capability   | [`capabilities/agent-server/`](capabilities/agent-server/README.md)     | `desktop.agentServer` — a local 127.0.0.1-only HTTP listener so external AI agents can call into the app |
| ProcessRunner capability | [`capabilities/process-runner/`](capabilities/process-runner/README.md) | `desktop.processRunner` — spawn an executable and stream its stdout/stderr back incrementally            |
| Microphone capability    | [`capabilities/microphone/`](capabilities/microphone/README.md)         | Webview `getUserMedia` + `MediaRecorder`, enabled by `package.json` "chain.permissions.microphone"       |
| Vision capability        | [`capabilities/vision/`](capabilities/vision/README.md)                 | `desktop.vision` — on-device text recognition (OCR) for image bytes                                      |
| Speech capability        | [`capabilities/speech/`](capabilities/speech/README.md)                 | `desktop.speech` — on-device transcription of a stored recording (SpeechAnalyzer via a Swift bridge)     |
| Models capability        | [`capabilities/models/`](capabilities/models/README.md)                 | `desktop.models` — user-downloaded, SHA-256-verified model packs run by the bundled sherpa-onnx engine   |
| Tts capability           | [`capabilities/tts/`](capabilities/tts/README.md)                       | `desktop.tts` — Kokoro/Piper voices via sherpa-onnx; GPL-3.0 opt-in per app (`chain.gpl`)                |
| AudioRecorder capability | [`capabilities/audio-recorder/`](capabilities/audio-recorder/README.md) | `desktop.audioRecorder` — record the microphone, the computer's own audio, or both mixed, to an AAC file |
| Browser capability       | [`capabilities/browser/`](capabilities/browser/README.md)               | `desktop.browser` — a separate signed-in browser window: own session, toolbar buttons, read the page, fetch with the session |
| Embeddings capability    | [`capabilities/embeddings/`](capabilities/embeddings/README.md)         | `desktop.embeddings` — on-device sentence embeddings from downloaded ONNX models + Hugging Face tokenizers |
| Window capability        | [`capabilities/window/`](capabilities/window/README.md)                 | `desktop.window` — title bar style and size, window buttons, appearance, background colour, drag regions, insets, full screen |
