# Audio recorder — microphone processing research

- mneme request 33: recording "both" through laptop speakers puts the
  computer's sound in the file twice, once from the tap and again tens of
  milliseconds later through the microphone. That echo also hurts
  transcription.
- mneme request 34: let the app choose noise suppression and automatic
  gain control too, for "microphone" and "both". Both are off by default.

All three are done by SpeexDSP in `crates/core/src/microphone_processor.rs`,
on the microphone only, before mixing. The system audio is never
processed.

## The shape of the problem

This is the textbook acoustic-echo-cancellation setup, and the far-end
reference comes for free: the system tap is exactly what the speakers
play. On macOS the microphone and the tap reach Rust in the same IO
cycle, sample-aligned and drift-compensated by the aggregate device
(research/MACOS.md). On Windows the WASAPI loopback stream plays the same
role. So the canceller is platform-independent Rust/C code that sits
between capture and `Mixer`. Only the capture under it differs per OS.

The echo lags the reference by output latency + acoustic path + input
latency (built-in speakers: a few tens of ms), then smears with the
room's reverberation. The reference always *leads* the echo, which is
what a causal adaptive filter needs.

## Options considered

- **WebRTC AEC3** (`webrtc-audio-processing` crate, 2.1). It has the best
  quality: delay estimation, nonlinear-echo handling and a strong
  residual suppressor. Building it needs meson, ninja and abseil (the
  `bundled` feature), or a system library through pkg-config. chain-core
  is compiled on every Chain app builder's machine (a git dependency),
  and `chain doctor` promises that only Rust is needed. Adding meson,
  ninja and a C++17 abseil build to that, on macOS and Windows, is the
  cost that rules it out for now.
- **SpeexDSP MDF** (`speex_echo_*`, plus `speex_preprocess` for residual
  echo). An adaptive multi-delay-block frequency-domain filter, ~200 KB
  of portable C89, BSD-3. The `aec-rs`/`speexdsp-sys` crates wrap it but
  need cmake and libclang (bindgen). Instead, **vendor the five source
  files and compile them with `cc`**, which needs only the C compiler a
  Tauri build already has. **Chosen.**
- **Apple's voice-processing IO unit** (`kAudioUnitSubType_VoiceProcessingIO`,
  `AVAudioInputNode.setVoiceProcessingEnabled`). Ruled out structurally,
  whatever its reference turns out to be:
  - It's its own IO unit on the default devices. It can't be an
    aggregate device's input, so the microphone would lose its sample
    alignment with the tap.
  - Its documented reference is the audio rendered through the unit
    itself. Other apps' audio is handled by *ducking*
    (`voiceProcessingOtherAudioDuckingConfiguration`, macOS 14), which
    lowers what the user hears and what the tap records. That matches
    mneme's belief, but it **wasn't tested here**: the structural reason
    alone is enough.
  - It also applies its own AGC and voice EQ, which colour the microphone.
  - macOS only, so Windows would need a second approach anyway.
- Small pure-Rust NLMS/FDAF crates (`fdaf-aec`, `rlx-aec`,
  `decibri-aec`, …): young, single-author, little track record. No
  advantage over Speex, which has been in VoIP stacks for 15+ years.

## What's built (`crates/core/src/microphone_processor.rs`)

- Vendored SpeexDSP 1.2.1: unmodified files from the official tarball
  (`downloads.xiph.org/releases/speex/speexdsp-1.2.1.tar.gz`, SHA-256
  `8c777343e4a6399569c72abc38a95b24db56882c83dbdb6c6424a5f4aeb54d3d`):
  `mdf.c preprocess.c fftwrap.c filterbank.c smallft.c` and their
  headers, in `crates/core/vendor/speexdsp/` with `COPYING`.
  `speexdsp_config_types.h` is hand-written (what `configure` generates).
- Compiled `FLOATING_POINT`, `USE_SMALLFT`. **Not kiss_fft**: sherpa-onnx
  already links a `kissfft` whose symbols would clash. Always `-O3`, even
  in debug builds.
- Frame: ~10 ms rounded up to a power of two (512 at 44.1/48 kHz, 256 at
  16 kHz). Tail: 0.2 s, rounded to whole frames. A delay longer than the
  tail isn't cancelled (wireless speakers can lag 150–300 ms).
- Speex's API is int16: f32 is converted on the way in and out.
- Residual echo: the preprocessor linked to the echo state. Turning
  denoise *off* also disables echo suppression (`preprocess.c` sets every
  gain to 1), so denoise stays on and "noise suppression off" means
  **noise suppression at 0 dB**. At 0 dB the gain floor without echo is
  ~1: the voice passes untouched and only residual echo is attenuated.
- IO-cycle blocks are buffered into frames, so the output lags the input
  by under one frame. Microphone and system leave together, still
  aligned. The remainder at stop (under ~10 ms) is dropped.
- Echo cancellation applies to "both" only. For a single source the
  option is ignored (CONTRACT.md).
- The processor exists only when at least one option is on for that
  source; otherwise the microphone is exactly as before (request 32).

## Noise suppression and gain control (request 34)

Both are the same Speex preprocessor that already runs after the echo
canceller, so this adds no dependency or cost.

- **Noise suppression**: Speex's spectral suppressor with a learned noise
  profile, at **−15 dB**, Speex's own default. It removes steady noise
  (fans, hum, hiss), not sudden sounds like typing or a door. The
  strength isn't an option: mneme's UI is on/off (rule 7). Stronger
  settings (−25 dB and below) leave "musical noise" and a processed,
  underwater voice, and −15 dB is a good balance. Adding a strength
  later is one `ctl` value.
- **Automatic gain control**: `Mixer`'s own microphone leveller (target
  −20 dBFS, 0.25×–8×, 1.5 s time constant), the same one "both" has
  always used. It was Speex's AGC at first, which turned out to be a
  bug (below). For "microphone" the option turns the leveller on; for
  "both" it's always on, so the option adds nothing there.

## The echo-gain bug (mneme, after request 34)

mneme heard echo in "both" with echo cancellation on. The separate
tracks (`CHAIN_RECORDER_TRACKS_DIR`, below) showed that the canceller
worked (22–30 dB off a −28 dBFS echo), but **two gain stages after it
turned the leftovers back up** while nobody spoke:

| no near-end speech, laptop speakers | after canceller | mic in the mix |
|---|---|---|
| echo only | −50 to −58 dBFS | −35 to −43 (mixer +15 dB) |
| echo + noise + Speex AGC | −37 to −46 (AGC +10 dB) | **−20 to −27** (mixer +17 dB) |

The lecture itself sits at about −20 dBFS in the mix, so with every
option on the leftover echo was as loud as the lecture.

The fix has one gain stage that is gated on near-end speech:

- Speex's AGC is gone. Its gain persists through pauses and multiplied
  the residual.
- `Mixer` adapts the microphone's gain only while someone is speaking.
  In pauses the applied gain eases (0.15 s) down to `min(gain, 1)`, so
  the residual is never raised. The learned level is kept for the next
  sentence, and the gain ramps across each block, so there are no steps.
- **Speaking = the canceller took off less than 15 dB** (smoothed
  before/after power per frame, in `MicrophoneProcessor::process`). Echo
  alone loses 20–30 dB. A voice the canceller has no reference for
  comes through almost whole. Without echo cancellation there's nothing
  to compare, so it says yes and levelling is by loudness, as before.
- **Speex's own VAD didn't work** for this. Replaying the real tracks, it
  called echo-only frames speech 90–100% of the time, even the room's
  silence. (It only reports a decision; it doesn't blank frames.)

Replaying the recorded tracks with a quiet voice added at 12–19 s
(`say -v Daniel`, ×0.25):

- Echo-only stretches counted as speech 0–25% of the time. Mic in the
  mix: **−37 to −55 dBFS** (it was −20 to −27).
- The added voice was detected 69–93% of the time and levelled to about
  −18 dBFS.
- The first 2–4 s of computer sound still let some echo through: the
  canceller hasn't adapted yet, so nothing can tell it from a voice.

Live take after the fix (same laptop, every option on, `say` lecture,
nobody speaking from 10 s): mic in the mix **−41 to −70 dBFS** against a
lecture at −20.
- **Dereverberation: left out.** In SpeexDSP 1.2.1 it does nothing:
  `SPEEX_PREPROCESS_SET_DEREVERB_LEVEL`'s setter is commented out
  ("Re-enable when de-reverberation is actually enabled again") and the
  reverb model is unused, so a switch would be a lie.
- **Voice activity detection** isn't an app option. Speex's VAD was
  tried internally for gain gating and failed (see the echo-gain bug
  above).
- "microphone" already goes through the native path (`desktop.audioRecorder`,
  request 32), so the options work there. They don't apply to the webview
  microphone capability (`getUserMedia`), which has the browser's own
  `noiseSuppression`/`autoGainControl` constraints.

## Measurements

Unit tests (synthetic, 48 kHz, noise "lecture", 30 ms delay plus a
10 ms decaying room response at −10 dB, fed in 441-frame blocks):

- Echo reduced by **77 dB** after adapting (asserted > 20 dB). A linear
  synthetic path is the best case: real speakers distort, so expect
  far less in a room.
- Voice with the computer silent: changed by −0.3 dB (asserted < 1 dB).
- Voice over the echo (double talk): within 0.1 dB (asserted > −6 dB).
- Noise suppression (300 Hz tone phrases over white noise): steady noise
  between phrases down **15.0 dB**, voice changed −0.1 dB.
- Gain control (then Speex's AGC, since replaced): a −43 dBFS voice
  raised +16 dB, but the noise between phrases rose the same amount.
  Now the mixer's leveller: unit-tested to raise a quiet microphone
  alone by more than 12 dB, and only when the option is on.
- All three on: 60 s of 48 kHz audio in 0.34 s.
- Speed: 60 s of 48 kHz audio in 0.29 s, debug build of the Rust side
  (C at -O3): ~200× real time, so a 3-hour lecture is no load. Memory is
  fixed: Speex state plus buffers of at most one frame plus one IO block.

Live (2026-10-04, MacBook Pro speakers and microphone, bundled
playground): 8 s of Gaussian noise played with `afplay` while recording
"both", each take converted to 8 kHz and autocorrelated over 6–9 s:

- `echoCancellation: false`: a clear peak at **46 ms**, 0.18. That's this
  laptop's speaker-to-microphone delay relative to the tap, well inside
  the 0.2 s tail.
- `echoCancellation: true`: **−0.005** at that lag, with no peak anywhere.
  The echo is below what this measurement can see.
- A `say` passage gives a peak at ~11.5 ms both ways: the voice's pitch
  period, not echo. Use noise to measure echo, not speech.

Live noise suppression (same setup, "microphone", 8 s of noise on the
speakers as a stand-in fan, level over 5–9 s):

- `noiseSuppression: false`: −35.5 dBFS. `true`: −49.7 dBFS, **−14.2 dB**,
  matching the −15 dB setting.
- Gain control wasn't measured live: it needs a real speaking voice.

## Debugging: separate tracks

Set `CHAIN_RECORDER_TRACKS_DIR=/some/folder` in the app's environment
(`CHAIN_RECORDER_TRACKS_DIR=… npm run dev`, or `open --env …` for a
bundle) and every take also writes `<time>-microphone.wav` (raw),
`-microphone-processed.wav`, `-microphone-in-mix.wav` (after the mixer's
gain) and `-system.wav`: mono 32-bit float, sample-aligned
(`crates/core/src/recorder_tracks.rs`). Compare the levels of the
processed or in-mix microphone in stretches where nobody speaks to see
what's left of the echo. It isn't part of the contract.

## Licensing

SpeexDSP is BSD-3-Clause (`vendor/speexdsp/COPYING`). Apps shipping
chain-core must include its notice alongside the others listed in
`agent-docs/capabilities/models/research/LICENSING.md`.

## Windows

Same Rust code. The loopback stream is the reference once WASAPI
capture exists (research/WINDOWS.md). It has to be resampled and aligned
to the microphone *before* the canceller: Speex tolerates a fixed delay
inside its tail but not clock drift between the two streams, which
breaks adaptation slowly over a long recording.
