# Tts capability

## How it works

An app that sets `"chain": { "gpl": true }` in its package.json gets
chain-core built with the `tts` feature (`packages/cli/src/features.ts`
adds `--features chain-core/tts` to `tauri dev`/`build`). That makes
`crates/core/build.rs` link sherpa-onnx's TTS-enabled static archive
(plus espeak-ng, piper_phonemize, ucd) instead of the no-TTS one, and
compiles the real engine in `crates/core/src/sherpa/tts.rs`.

`desktop.tts.synthesize` → `tts_synthesize` (`templates/lib.rs`) resolves
the config's files and folders inside the installed model
(`Models::resolve`/`resolve_dir`), then on a blocking thread loads the
model (cached; a one-word warm-up runs at load), generates samples, wraps
them as a 16-bit WAV, and stores it with `files.write` — the app gets a
reference. Without the feature every command rejects `UNSUPPORTED`.

`desktop.tts.compile` → `tts_compile` speaks each segment in turn
(`sherpa::tts::compile`, engine lock taken per segment so `synthesize`
can interleave) and streams the samples into `m4a::M4aWriter` — AAC,
64 kbps mono, through AudioToolbox's ExtAudioFile — at a temp path.
Timings are summed from sample counts. Only on success does
`Files::adopt` move the file into the managed directory; on cancel or
failure the temp file is removed. Progress goes out as
`chain://tts-progress`; `tts_cancel` sets a flag checked between segments.

The loaded model (about 600 MB for Kokoro) stays in `LOADED` until it's
freed. `desktop.tts.setIdleUnload(ms)` → `set_idle_unload` starts one
reaper thread that frees it once no call has finished for `ms`: every
`with_engine` call and the end of a compile stamp `LAST_USED`; the
reaper only frees when it can take `LOADED` without waiting (a call
holding it is in use) and no compile is running. A new setting wakes it
through a condvar, so it applies at once. `desktop.tts.unload()` →
`unload` waits out a running compile, then takes `LOADED` (waiting for a
call in progress) and frees it (mneme request 29).

## How to use it

```json
// package.json — accepts GPL-3.0 for this app's releases
"chain": { "gpl": true }
```

```ts
const kokoro = {
  type: "kokoro",
  model: "model.onnx",
  voices: "voices.bin",
  tokens: "tokens.txt",
  dataDir: "espeak-ng-data",
  dictDir: "dict",
  lexicon: ["lexicon-us-en.txt", "lexicon-zh.txt"],
  speakers: KOKORO_V1_NAMES // 54 names, from the model's own metadata
} as const;
const voices = await desktop.tts.voices("kokoro-multi-lang-v1-0", kokoro);
const wav = await desktop.tts.synthesize(paragraph, {
  modelId: "kokoro-multi-lang-v1-0",
  config: kokoro,
  voice: 3
});
audio.src = await desktop.files.url(wav);

// Audiobook mode: only when the user asks (e.g. a "Download audio" button).
const book = await desktop.tts.compile(sentences, { modelId, config: kokoro, voice: 3 }, (f) =>
  setProgress(f)
);
// Save book.file and book.segments with the page. Highlight by time:
audio.src = await desktop.files.url(book.file);
audio.ontimeupdate = () => highlight(book.segments.findIndex((s) => audio.currentTime < s.end));
// "Remove audio": desktop.files.delete(book.file)

// Memory: free the model after a minute without use (any time; 0/null = never),
// or right away, e.g. when the user turns read-aloud off.
await desktop.tts.setIdleUnload(60_000);
await desktop.tts.unload();
```

## Files to check

- `capabilities/tts/contract.ts` — TS types.
- `crates/core/src/sherpa/tts.rs` — configs, voice naming, WAV writer, engine cache and its idle reaper, `compile` + timings (+ tests incl. layout).
- `crates/core/src/m4a.rs` — the AAC writer (macOS ExtAudioFile; `Unsupported` elsewhere) + test.
- `crates/core/src/files.rs` — `Files::adopt`, how a finished compile becomes a reference.
- `crates/core/src/sherpa/tts_ffi.rs` — C bindings (feature-gated).
- `crates/core/build.rs` — TTS archive table and extra libs.
- `packages/cli/src/features.ts` — the `chain.gpl` opt-in.
- `packages/cli/templates/lib.rs` — `tts_voices`/`tts_synthesize`/`tts_compile`/`tts_cancel`/`tts_set_idle_unload`/`tts_unload`.
- `packages/sdk/src/tts.ts` — SDK wrapper.
- `agent-docs/capabilities/models/research/LICENSING.md` — what the opt-in links.
