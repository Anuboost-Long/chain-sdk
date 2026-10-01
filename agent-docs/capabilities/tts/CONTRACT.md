# Tts Capability — Contract

## What this is

On-device text-to-speech with voice models the user downloaded through
`desktop.models` (Kokoro, Piper/VITS, Kitten), run by sherpa-onnx compiled
into the app. Requested by mneme (request 21, part 2) for read-aloud.

**Opt-in, because of GPL-3.0.** sherpa-onnx's TTS links espeak-ng
(GPL-3.0) for pronunciation. It's only compiled in when the app declares

```json
"chain": { "gpl": true }
```

in its own `package.json` — `chain dev`/`chain build` then build
chain-core with its `tts` feature. By setting it, the app accepts GPL-3.0
obligations for its releases. Without it, both methods reject
`UNSUPPORTED` and the app contains no GPL code. See
`agent-docs/capabilities/models/research/LICENSING.md`.

## `desktop.tts.voices(modelId, config)`

Resolves `[{ id, name, language? }]`, one per speaker in the model, in id
order. Names come from `config.speakers` (the catalog's list — model files
don't carry names), else `"Voice N"`. `language` is filled for Kokoro
voices whose names follow Kokoro's convention (`af_…` → `en-US`, `bf_…` →
`en-GB`, `zf_…` → `zh`, …). Loads the model if it isn't loaded.

## `desktop.tts.synthesize(text, { modelId, config, voice?, speed? })`

Speaks `text` with speaker `voice` (default 0) at `speed` (0.25–4, default

1. and resolves a `desktop.files` reference to a 16-bit mono WAV at the
   model's sample rate. Play it with `files.url()`; delete it with
   `files.delete()` when done — the app owns it like any stored file.

- One model stays loaded between calls (switching models reloads), so
  paragraph-by-paragraph synthesis doesn't pay the load cost each time —
  until it's unloaded (see `setIdleUnload` below).
- Calls run one at a time; a second call waits for the first.
- No streaming: the whole WAV exists when the promise resolves. Apps split
  long text into paragraphs themselves.

## `desktop.tts.compile(segments, options, onProgress?)`

For listening offline, e.g. an audiobook mode: speaks every string in
`segments`, in order, into **one** AAC `.m4a` and resolves

```ts
{ file, duration, segments: [{ start, end }, …] }
```

- `file` — a `desktop.files` reference. Play it with `files.url()`; it's
  the app's like any stored file, and `files.delete(file)` removes it
  completely. Nothing else is kept anywhere.
- `segments[i]` — where input `segments[i]` sits in the audio, in seconds,
  from exact sample counts. Back to back: `segments[i].end ===
segments[i + 1].start`, the first starts at 0, the last ends at
  `duration`. Each segment's own trailing pause (what the voice model
  produces) belongs to it. To highlight along playback, find the segment
  whose range holds `audio.currentTime`.
- `options` — the same as `synthesize` (`modelId`, `config`, `voice`,
  `speed`); one voice for the whole recording.
- `onProgress(fraction)` — 0–1 by characters spoken so far.

The app decides what a segment is (a sentence, a heading, a list item) —
Chain never splits text, so the app's own mapping from segment back to
the text on screen stays exact.

**Only when asked.** It runs only when the app calls it (e.g. from a
"Download audio" button) and takes a while (Kokoro speaks about 4× faster
than real time on an M3 Pro, so a 10-minute page takes a few minutes).
Until it resolves nothing is visible: the audio is written to a temporary
file and becomes a `desktop.files` reference only on success. If it's
cancelled or fails, the temporary file is removed.

- One compile at a time; another call while one runs rejects
  `UNAVAILABLE`. `synthesize` calls can run in between segments, so
  read-aloud still works during a compile.
- `desktop.tts.cancel()` stops it; it rejects `CANCELLED` after the
  segment being spoken finishes. A no-op when no compile is running.
- About 0.5 MB per minute (AAC, mono, 64 kbps), against 2.9 MB for WAV.

## Freeing the model — `setIdleUnload(ms)` and `unload()` (mneme request 29)

```
setIdleUnload(ms: number | null): Promise<void>
unload(): Promise<void>
```

A loaded model holds hundreds of MB (about 600 MB for Kokoro), so an app
that warms one up ahead of use can let it go again:

- `setIdleUnload(ms)` frees the loaded model once no call has used it
  for `ms` milliseconds. Call it whenever: a new value applies at once,
  still counted from the last call (so lowering it below the time
  already idle frees the model right away). `0` or `null` means never —
  the default. It lasts until the app quits, across webview reloads.
  `ms` must be a whole number ≥ 0 (else `INVALID_ARGUMENT`).
- `unload()` frees it and resolves once it's freed; a no-op when nothing
  is loaded.
- Neither ever interrupts a call. A `voices`/`synthesize` call in
  progress keeps the model loaded until it finishes, and a running
  `compile` counts as in use from start to end — `unload()` waits for
  either to finish first. The idle time counts from when the last call
  finished.
- The next call after an unload loads the model again, as the first one
  did (about a second for Kokoro).
- "Freed" means the engine is destroyed. How much of its memory the OS
  gets back right away is up to the system allocator: on macOS Chain asks
  for it immediately (`malloc_zone_pressure_relief`), and anywhere from
  all of it to about half comes back; the rest stays reserved in the
  process and is reused by the next load, so repeated load/unload cycles
  don't grow memory.
- The timer runs natively, so it sees every call, from any window.

## `TtsModelConfig`

File and folder names relative to the model, from the app's catalog:

- `kokoro` — `model`, `voices`, `tokens`, `dataDir` (espeak-ng-data),
  optional `dictDir` (jieba, Chinese), `lexicon[]`, `lang`, `speakers[]`.
- `vits` (Piper) — `model`, `tokens`, `dataDir`, optional `dictDir`,
  `lexicon[]`, `speakers[]`.
- `kitten` — `model`, `voices`, `tokens`, `dataDir`, `speakers[]`.

## Errors (`ChainErrorCode`)

- `UNSUPPORTED` — the app hasn't opted in (`chain.gpl`), the platform has
  no engine (for `compile`: no AAC encoder yet — Windows, see
  research/WINDOWS.md), or the call ran outside a Chain app.
- `NOT_FOUND` — the model, or a named file or folder in it, isn't installed.
- `INVALID_ARGUMENT` — empty text (for `compile`: no segments, or a blank
  one, named by its index), a voice id the model doesn't have, a speed
  outside 0.25–4, a name that isn't a plain relative path, or an
  `setIdleUnload` time that isn't a whole number ≥ 0.
- `UNAVAILABLE` — `compile` while another compile is running.
- `CANCELLED` — `compile` stopped by `cancel()`.
- `NATIVE_FAILURE` — the model failed to load (files don't match `type`)
  or synthesis failed.

## Non-goals

- No streaming playback, no SSML, no word timings — `compile` times
  whole segments (the engine returns samples only; see
  research/MACOS.md for what word timings would take).
- `compile` doesn't split text, pick a format, or mix voices. AAC only.
- No resume: a cancelled compile starts over.
- No voice cloning (ZipVoice, Pocket) or Matcha (needs a separate vocoder)
  until an app asks.
- Doesn't choose voices or download anything — `desktop.models` and the
  app's catalog do that.
