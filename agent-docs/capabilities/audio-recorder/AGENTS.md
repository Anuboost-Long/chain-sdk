# AudioRecorder Capability — Agent Memory

Scope: `desktop.audioRecorder` — record the microphone, the computer's
own output, or both mixed into one AAC file. Requested by mneme (request
32, `docs/chain-sdk-requests/32-system-audio-capture.md` in the mneme
repo) for recording lectures and calls the laptop plays.

## What's already decided

- **Native capture for all three sources**, so mneme has one recording
  path. The webview microphone route (`agent-docs/capabilities/microphone/`)
  stays as the fallback where `availability().microphone` is false.
- **Core Audio process tap, not ScreenCaptureKit**: audio-only permission
  instead of Screen Recording. macOS 14.2+ for system audio.
- **One aggregate device** carries both sources, so mixing needs no
  resampler or drift handling of our own.
- **The app's own sound is included** (a global tap). Excluding it would
  mean finding WebKit's GPU process, since webview audio plays there.
- **Tap auto-start is off** (`kAudioAggregateDeviceTapAutoStartKey:
  false`): with it on, the device stops its IO cycle while nothing plays
  and silent stretches vanish from the file.
- **Private TCC SPI** for system-audio permission (no public API, and a
  refused tap records silence). Fine outside the Mac App Store.
- Mixing logic lives in Rust (`Mixer`, unit-tested); Swift only captures.
- **Echo cancellation (request 33) is vendored SpeexDSP, compiled with
  `cc`**, not WebRTC AEC3 (needs meson/ninja/abseil on every builder) and
  not Apple voice processing (can't take an aggregate device's input,
  ducks other audio). On by default for "both", ignored for one source.
  Residual suppression runs with noise suppression at 0 dB, since
  denoise off disables echo suppression too. Use smallft, never Speex's
  kiss_fft (it clashes with sherpa-onnx's). See research/PROCESSING.md.
- **Noise suppression (request 34)** is the same Speex preprocessor, on
  the microphone for "microphone" and "both", ignored for "system", off
  by default. **Gain control is `Mixer`'s levelling, never Speex's AGC.**
  Its gain persisted through pauses and raised the leftover echo to
  lecture level. The microphone's gain only applies while someone speaks
  (the canceller removed < 15 dB); otherwise it's ≤ 1×. Don't add a
  second gain stage before the mixer. Speex's VAD can't tell residual
  echo from a voice, so don't use it for gating. Noise suppression is fixed at −15 dB (no
  strength option until an app needs one). Dereverb isn't offered: it's
  a no-op in SpeexDSP 1.2.1. The processor is only created when an
  option is on, so default recordings are unchanged.
- **Microphone choice (request 35)**: by device UID, never by changing the
  system default. A default change never moves a take; only the
  recording device disappearing does (rebuild on the replacement, same
  tap, event to the app, Rust converts the rate). Explicit id beats
  `avoidBluetoothMicrophone`; an id not connected → `UNAVAILABLE`.
- Don't grow this into output-device picking, per-app capture or
  multitrack without a new real requirement (CONTRACT.md's non-goals).

## Status

- [x] macOS 27.0.1 (Apple Silicon, AirPods as input and output),
      bundled `apps/playground` debug build with the dev inspector,
      2026-10-04:
      - availability → all true with both permissions declared.
      - system: a YouTube video plus `say` transcribed back by
        `desktop.speech`; 48 kHz mono AAC, `durationMs` = `afinfo`.
      - microphone: AirPods mic in call mode, 16 kHz — found and fixed
        `M4aWriter` failing every write ('!dat') at 8/16 kHz.
      - both: computer sentence + the user's spoken sentence both in the
        transcript.
      - pause: no levels while paused, span left out of duration.
      - silence: a quiet stretch is kept (duration = wall time).
      - start while recording / stop when idle → `UNAVAILABLE`; cancel →
        no file added to files; reload cancels; bad source →
        `INVALID_ARGUMENT`.
- [x] `cargo test -p chain-core` (mixer, meter, encoder thread, rates).
- [x] Echo canceller unit tests (synthetic echo, 48 kHz, IO-sized
      blocks): 77 dB echo reduction once adapted, voice kept within
      0.3 dB alone and 0.1 dB over the echo; ~200× real time.
- [x] Noise suppression / gain control unit tests: noise −15 dB, voice
      −0.1 dB; quiet voice +16 dB without clipping; with both on, noise
      between phrases only +1.4 dB. All three on: ~175× real time.
- [x] Noise suppression live, 2026-10-04, MacBook Pro mic, bundled
      playground: availability reports all six flags true; "microphone"
      while 8 s of noise played on the speakers: off −35.5 dBFS, on
      −49.7 dBFS (−14.2 dB). "both" with all three options on records
      cleanly.
- [x] Echo-gain bug (mneme): measured with the debug tracks, fixed with
      speech-gated gain. Replay of the real tracks: leftover echo in the
      mix −37 to −55 dBFS (was −20 to −27); live take −41 to −70 against a
      −20 lecture. research/PROCESSING.md.
- [ ] Gain control live (a quiet, distant voice), noise suppression on a
      real fan, and real double talk over a lecture, judged by ear.
- [ ] Refusal → `PERMISSION_DENIED`: couldn't produce a clean refusal for
      the unsigned debug build (its grant survived `tccutil reset` and
      removal in Settings). Verify with a signed build or a fresh Mac
      user.
- [ ] Signed, hardened-runtime build.
- [x] Microphone disappearing mid-recording (2026-10-04): a test
      aggregate input destroyed 4 s into "both" → moved to the built-in
      mic, change event, file full length. `microphones()` lists built-in,
      Continuity ("other") and the test device; unknown id → UNAVAILABLE.
- [ ] Real Bluetooth (AirPods): transport "bluetooth"; default AirPods mic
      + `avoidBluetoothMicrophone` → built-in mic, headset stays A2DP (what
      the user hears doesn't drop); without it, "both" at 16 kHz confirms
      the tap degradation; AirPods disconnecting mid-take → event.
- [ ] Output device disappearing during "system" (moves to the new
      default output) — not tested.
- [ ] Multi-hour recording (memory flat, file finishes).
- [x] Echo cancellation live, 2026-10-04, MacBook Pro speakers + mic,
      bundled playground: `availability().echoCancellation` true; 8 s of
      noise played while recording "both" — off: echo peak at 46 ms
      (autocorrelation 0.18), on: −0.005 at that lag (gone). Speech
      (`say`) can't be measured this way: its pitch period dominates.
- [ ] Echo cancellation with real double talk (a person talking over a
      lecture on speakers), judged by ear and by transcription.
- [ ] Windows: not started — research/WINDOWS.md.
