# Models Capability — Agent Memory

Scope: `desktop.models` (install/cancel/list/remove data-only model packs)
and the bundled sherpa-onnx engine behind `desktop.speech.transcribe`'s
`engine` option. Requested by mneme
(`docs/chain-sdk-requests/21-model-extensions.md` in the mneme repo).

## What's already decided (with the user, 2026-09-28)

- **Models are user downloads; the engine is compiled into the app.**
  Nothing downloaded is executable — macOS won't trust runtime-downloaded
  code, and the request forbids it.
- **No GPL by default**: sherpa-onnx's official no-TTS static libs. The
  TTS-enabled archive (espeak-ng, GPL-3.0) is linked only with chain-core's
  `tts` feature, which only an app's own `"chain": { "gpl": true }` turns
  on (user decision, 2026-09-28). Never make it the default, and never use
  the `sherpa-onnx`/`sherpa-onnx-sys` crates (they always link TTS). See
  research/LICENSING.md.
- **TTS** lives in `crates/core/src/sherpa/tts.rs` — see
  `agent-docs/capabilities/tts/`.
- VAD is a separately installed model (Silero, 0.6 MB), not bundled —
  the user asked for nothing model-like inside the app.
- Segments: VAD cuts speech, then anything over the family's limit
  (Moonshine 8 s — its v2 decoder fails past ~9 s; others 20 s) is split at
  the quietest 20 ms frame in the back half of the window.
- Audio decoding with symphonia on every OS. **It has no Opus decoder**, so
  WebView2's default `audio/webm;codecs=opus` recordings can't be
  transcribed yet (see speech/research/WINDOWS.md).

## Status

- [x] macOS arm64: `cargo test -p chain-core models sherpa` (hostile tars,
      layout vs `c-api.h`, splitting); probe + end to end in
      `apps/playground`: install with progress, cancel mid-download (nothing
      left in `.partial`), `INTEGRITY_FAILED`, `http://` rejected, engine
      transcription with Moonshine tiny-en + Silero, `NOT_FOUND` for a missing
      model/file, idempotent remove. 60-min lecture: 57 s, 697 segments,
      no decoder failures.
- [ ] Windows x64: archive pinned (MD CRT) but never built or linked — the
      static-lib names and system libs it needs are unconfirmed.
- [ ] Whisper / SenseVoice / Paraformer / transducer / NeMo configs are
      wired but only Moonshine has run.
- [ ] Opus decoding for WebView2 recordings.
- [ ] Contract tests under `capabilities/models/tests/`.
