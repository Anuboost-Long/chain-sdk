# Tts Capability — Agent Memory

Scope: `desktop.tts.voices(modelId, config)` and
`desktop.tts.synthesize(text, { modelId, config, voice, speed })` over
sherpa-onnx's offline TTS, with voices installed through `desktop.models`.
Requested by mneme (request 21, part 2).

## What's already decided (user, 2026-09-28)

- **GPL-3.0 only by per-app opt-in.** sherpa-onnx TTS links espeak-ng.
  chain-core's `tts` Cargo feature swaps in the TTS-enabled archive; only
  an app's own package.json `"chain": { "gpl": true }` enables it
  (`packages/cli/src/features.ts` → `--features chain-core/tts`). Every
  other Chain app stays GPL-free. Never make it a default.
- The commands exist in every build (the template's `generate_handler!`
  is static); without the feature they reject `UNSUPPORTED` naming the flag.
- One loaded model is cached (`LOADED` in `sherpa/tts.rs`), keyed by the
  resolved config, so read-aloud doesn't reload per paragraph.
- Output: 16-bit mono WAV written through the files capability; the app
  gets a reference.
- Voice names come from the catalog (`config.speakers`) — sherpa-onnx's
  C API exposes only a speaker count.

## Status

- [x] macOS arm64 (M3 Pro), end to end in `apps/playground` built with
      `--features chain-dev-inspector,chain-core/tts`:
      Piper `en_US-amy-low` and Kokoro multi-lang v1.0 installed through
      `desktop.models`; `voices()` (54 Kokoro voices with languages);
      synthesis in US/GB English and Chinese; English output transcribed
      back word for word by `desktop.speech`; speed; all four error paths.
      Kokoro: load ~1 s (with warm-up), then ~4× real time (2.5 s for an
      11 s paragraph); without the warm-up the first paragraph took 12 s.
      Piper amy-low: ~40× real time.
- [x] `cargo test -p chain-core` with and without `--features tts`
      (layout checked against `c-api.h`: config 448 bytes).
- [x] Bundle: TTS adds ~1.1 MB to the release binary (0.5 MB gzipped) on
      top of the ASR engine.
- [x] `compile` (audiobook mode, 2026-09-29), macOS arm64, playground
      with Kokoro: 4 sentences → one 12 s `.m4a` (AAC 65 kbps, 131 KB),
      back-to-back timings from sample counts; the webview's
      `audio.duration` equals the computed duration exactly (the
      encoder's 2112 priming frames are trimmed on playback, so timings
      don't drift). Cancel mid-compile → `CANCELLED` with no file and
      no temp left; a second concurrent call → `UNAVAILABLE`;
      `files.delete` removes the result. Progress reaches 1.
- [x] Idle unload / `unload()` (mneme request 29, 2026-10-01), macOS
      arm64, in mneme's running window with Kokoro, measured with
      `vmmap --summary`'s physical footprint (RSS overstates it on macOS:
      freed-but-reusable pages still count). Baseline 37 MB, loaded
      637 MB. A 3 s idle timeout didn't free at 1.5 s idle and had by
      4.5 s; lowering 60 s → 1 s while already 2 s idle freed at once;
      `unload()` twice is a no-op the second time; `-5` →
      `INVALID_ARGUMENT`. That the engine really is gone was confirmed by
      timing: the next `voices()` took 3.85 s (a load) against 6 ms when
      loaded. Memory returned varies: without
      `malloc_zone_pressure_relief` it came back over 5–10 s; with it,
      one `unload()` went 637 → 50 MB by the time it resolved, while
      other cycles kept ~330 MB reserved, which the next load reused
      (loaded footprint 475 MB, not 637) — no growth across cycles.
- [ ] Idle unload on Windows: same code, but no `return_freed_memory`
      equivalent (the Windows heap decommits on its own schedule); unrun.
- [ ] `compile` on Windows: `M4aWriter` is `Unsupported` there — see
      research/WINDOWS.md for the Media Foundation plan.
- [ ] Windows: never built.
- [ ] Kitten voices: wired, never run.
