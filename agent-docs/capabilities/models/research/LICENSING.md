# Models — licensing of the bundled engine

Decided 2026-09-28 with the user: the engine ships in the app, models are
user downloads, and **no GPL code is linked by default**. Later the same
day the user approved GPL-3.0 TTS **as a per-app opt-in** (mneme's user
accepted GPL-3.0 for mneme): only an app whose package.json says
`"chain": { "gpl": true }` links the TTS-enabled build. See "GPL opt-in"
below.

## What sherpa-onnx's builds contain

- Every standard prebuilt archive (static and shared) compiles in
  `piper_phonemize` and `espeak-ng` (GPL-3) — even for ASR-only use. The
  `sherpa-onnx-sys` crate links them unconditionally
  (`SHERPA_ONNX_STATIC_LIBS` includes `espeak-ng`, `piper_phonemize`,
  `ucd`). "Dynamic linking" doesn't separate them: the GPL code sits inside
  the shared library itself.
- sherpa-onnx also publishes **`*-static-no-tts-lib`** archives, built with
  `-DSHERPA_ONNX_ENABLE_TTS=OFF`. CMake then never fetches espeak-ng or
  piper-phonemize (`CMakeLists.txt`: `if(SHERPA_ONNX_ENABLE_TTS)
include(espeak-ng-for-piper) …`).
- Verified on `osx-arm64-static-no-tts-lib` 1.13.8: no espeak/piper
  archives, zero undefined `espeak_*` references. (A naive `grep -i espeak`
  over the symbol table matches `OfflineSpeakerDiarization` — "Offlin**eSpeak**er".)

## What Chain links (crates/core/build.rs)

sherpa-onnx 1.13.8 no-TTS static libs, pinned by SHA-256 per target:
sherpa-onnx core + C API (Apache-2.0), kaldi-native-fbank, kaldi-decoder,
kaldifst/OpenFST (Apache-2.0), kissfft (BSD-3), simple-sentencepiece
(Apache-2.0), ONNX Runtime (MIT). The FFI declarations were copied from
`sherpa-onnx-sys` (Apache-2.0) instead of depending on it, because that
crate links the TTS libraries. Audio decoding: symphonia (MPL-2.0 —
file-level copyleft; used unmodified, so only its notice applies).

Apps shipping this must include the Apache-2.0/MIT/BSD notices of these
components in their about box or license file.

## GPL opt-in (`"chain": { "gpl": true }`) — what it adds

`chain dev`/`chain build` pass `--features chain-core/tts`
(`packages/cli/src/features.ts`). chain-core's build script then links
sherpa-onnx's standard TTS-enabled static archive instead of the no-TTS
one — same pinned version, its own SHA-256 — plus three more libraries:

- `espeak-ng` — **GPL-3.0-or-later**. Text-to-phoneme for Piper, Kokoro
  (non-lexicon languages) and Kitten voices. Voices also need its
  `espeak-ng-data` folder, which ships **inside the downloaded model**
  archives (data files, same license) — not in the app.
- `piper_phonemize` — MIT, but calls espeak-ng.
- `ucd` — espeak-ng's Unicode tables (GPL-3.0 with espeak-ng).

An app built this way is a combined work with GPL-3.0 code: distributing
it means offering its complete corresponding source under GPL-3.0 and
keeping every license notice. Apps without the flag are unaffected, and
the flag can't be turned on by a dependency — only the app's own
package.json.

## TTS options without GPL (not built)

1. Kokoro-82M (Apache-2.0 weights) with an English pronunciation step
   written for Chain (dictionary-based, like misaki's MIT lexicons),
   feeding Kokoro's ONNX model directly — not through sherpa-onnx's TTS,
   which would bring espeak-ng back.
2. A model that reads text directly with no phonemizer (e.g. Supertonic —
   check its model license terms first).
3. Accept GPL-3 — only if the app itself is GPL-compatible.

## Model licenses (from mneme's catalog review, 2026-09-28)

Model weights carry their own licenses, separate from the engine's. The
app's catalog must list only models it may ship to its users:

- Moonshine **English** (tiny, base) — MIT. Moonshine's **non-English**
  models use the non-commercial Moonshine Community License; exclude them
  from commercial apps.
- Whisper (base, small, …) — MIT.
- Silero VAD — MIT.
- SenseVoice `2025-09-09` is converted from a third-party Cantonese model
  (ASLP-lab WSYue-ASR); confirm that license before listing it.

- Kokoro-82M v1.0 — Apache-2.0 weights (its espeak-ng-data folder is
  GPL-3.0 data, like the engine's espeak-ng).
- Piper voices — per-voice dataset licenses. `vits-piper-en_US-amy-low`'s
  MODEL_CARD only says "License: See URL" (mimic3-voices); check each
  voice's dataset before listing it.

sherpa-onnx's release pages don't state per-model licenses. Check each
upstream model card; don't assume the engine's Apache-2.0 covers the weights.
