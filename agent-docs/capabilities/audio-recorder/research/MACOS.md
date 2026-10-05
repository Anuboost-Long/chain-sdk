# Audio recorder — macOS research

## Why native

WKWebView reaches input devices only: no `getDisplayMedia` audio, no
loopback. The computer's own output has to be captured natively (mneme
request 32).

## Options for the computer's output

- **Core Audio process taps** (macOS 14.2+): `CATapDescription` +
  `AudioHardwareCreateProcessTap`, read through a private aggregate
  device. Audio only, its own TCC service (`kTCCServiceAudioCapture`,
  "System Audio Recording Only" in Privacy & Security), usage sentence
  `NSAudioCaptureUsageDescription`. **Chosen.**
- ScreenCaptureKit (macOS 13+): needs Screen Recording permission, whose
  prompt and Settings row talk about recording the screen — wrong thing
  to ask for an audio recorder.
- Virtual loopback drivers (BlackHole, …): a kernel/HAL plug-in the user
  installs. Not something an app can ship.

## How capture works (`crates/core/swift/ChainRecorder.swift`)

One private aggregate device per recording, whatever the source:

- **Microphone**: sub-devices = [default input device].
- **Computer audio**: sub-devices = [default output device] (the clock),
  plus a global stereo tap (`stereoGlobalTapButExcludeProcesses: []`,
  `muteBehavior = .unmuted`, private).
- **Both**: sub-devices = [default input device], plus the tap with
  drift compensation. The aggregate resamples and drift-compensates the
  tap against the microphone's clock, so the two arrive in one IO
  callback, frame-aligned, at one rate — no resampler or ring buffer of
  our own.

In the IO callback's input buffer list the sub-devices' input streams
come first, the tap's last; Swift downmixes each source to mono and
hands Rust two plain `f32` arrays (or one) plus the aggregate's sample
rate. The IO block runs on a serial dispatch queue (not the real-time
thread), so calling into Rust and allocating there is fine.

The aggregate's rate is its first sub-device's: the microphone's for
microphone and both (Bluetooth headsets in call mode can be 16/24 kHz),
the output device's for computer audio. AAC takes all of those.

Rust (`crates/core/src/audio_recorder.rs`) mixes, meters and encodes
on a worker thread into `m4a.rs`'s `M4aWriter` (AAC 64 kbps mono,
ExtAudioFile), so nothing accumulates in memory: about 0.5 MB a minute,
~90 MB for three hours.

## Permissions

- Microphone: `AVCaptureDevice.authorizationStatus(for: .audio)` /
  `requestAccess` before starting. Same TCC service and usage sentence
  as the webview microphone (`NSMicrophoneUsageDescription`).
- Computer audio: there's no public preflight or request API. Without
  permission the tap still starts but delivers silence, so a refusal
  can't be seen from the audio. The private `TCC.framework`'s
  `TCCAccessPreflight` / `TCCAccessRequest` with
  `kTCCServiceAudioCapture` are looked up with `dlsym` (the same
  approach as Apple-sample-derived tools such as AudioCap). Missing
  symbols → we skip the check and let the tap prompt on its own.
  Private API: fine outside the Mac App Store, would need revisiting for
  it.
- Requesting either one without its usage sentence in Info.plist makes
  TCC kill the process, so the bridge checks
  `Bundle.main.object(forInfoDictionaryKey:)` first and reports the
  source unavailable instead. `chain dev` embeds the plist in the binary
  (`__TEXT,__info_plist`), where `Bundle.main` finds it.
- No entitlement is needed for system audio under the hardened runtime;
  the microphone keeps `com.apple.security.device.audio-input`.

## Mixing (both)

Each source has a slow automatic gain (target −20 dBFS RMS, 0.25×–8×,
~1.5 s time constant, held while the source is below −50 dBFS so room
hiss isn't pumped up), then the sum goes through a soft limiter above
0.8. A quiet laptop-microphone voice and a loud lecture end up at a
similar level. Microphone or computer audio alone is recorded as is.

Speakers + microphone: the microphone also hears the speakers tens of
ms late. With `echoCancellation` (default on) that echo is cancelled
before levelling, with the tap as the reference: SpeexDSP in Rust, not
Apple's voice processing, which can't take an aggregate device's input.
See research/PROCESSING.md, which also covers noise suppression and gain
control on the microphone.

## Choosing the microphone (request 35)

- `inputs()` lists `kAudioHardwarePropertyDevices` with input streams,
  minus our own aggregates (UID prefix `chain-recorder-`). Transport is
  `kAudioDevicePropertyTransportType`: built-in; Bluetooth/BluetoothLE
  → "bluetooth"; USB; everything else (aggregates, virtual devices,
  the iPhone's Continuity mic) → "other". The id is the device UID,
  which is stable across launches.
- The aggregate has always been built on one device UID, so the default
  input changing (headphones connecting) never moved a take. It's now
  that UID, the chosen one, or the avoid-Bluetooth replacement. Nothing
  touches the system default.
- **Bluetooth headset mics and quality.** Opening the input switches the
  headset from A2DP to HFP. The AirPods mic then runs at 16 kHz (seen
  in request 32's testing). In "both" the aggregate's clock and rate are
  the microphone's, so **the tap is resampled to that rate too**: the
  computer's audio in the file drops to call quality along with the
  voice. That's by construction, not measured with AirPods here. With
  the built-in mic chosen, the headset's *output* stays A2DP (only the
  input opens HFP) and the file runs at 48 kHz. Needs a live check
  with AirPods (AGENTS.md).
- **Disappearing device**: a listener on `kAudioHardwarePropertyDevices`
  (serial queue `chain.audio-recorder.devices`) checks
  `kAudioDevicePropertyDeviceIsAlive` on the aggregate's clock device.
  If it's dead, the aggregate is destroyed and rebuilt on the
  replacement with the **same process tap**, the same Rust context and
  callback. Rust gets the change as an event on its worker thread, and
  converts the rate if the new device's differs (`RateConverter`, linear).
- Verified 2026-10-04 with a public aggregate "Chain Test Mic" (the
  built-in mic wrapped) created by a helper and destroyed 4 s into a
  "both" take: the take moved to the MacBook Pro Microphone, the event
  arrived (previous = Chain Test Mic), and the file kept its full length
  (10.3 s). Unknown id → `UNAVAILABLE`; "system" → no microphone in the
  result.

## Read-aloud

The tap is global, so it includes everything the computer plays —
including the app's own read-aloud. Excluding it isn't just excluding
our pid: webview audio plays from WebKit's GPU process.

## Findings from testing (2026-10-04, macOS 27.0.1)

- **Tap auto-start must be off.** With
  `kAudioAggregateDeviceTapAutoStartKey: true` the IO callback simply
  isn't called while nothing plays, so a silent stretch disappears from
  the file and the duration. With it off the device runs continuously.
  The callback also zero-fills any source whose buffer comes back empty.
- **AAC at 8/16 kHz can't take 64 kbps.** AirPods' microphone in call
  mode runs at 16 kHz; asking the encoder for 64 kbps there makes every
  `ExtAudioFileWrite` fail `'!dat'`. `m4a.rs` now picks the highest
  applicable rate ≤ 64 kbps (`kAudioConverterApplicableEncodeBitRates`).
  This also affected tts `compile` with 16 kHz Piper voices.
- **Every rebuild re-prompts** for both microphone and system audio:
  the unsigned debug build's grant is tied to its code signature. While
  a prompt is open, `start` stays pending.
- **Refusal couldn't be reproduced** on the debug build: after
  `tccutil reset AudioCapture <bundle id>`, removing the row in Settings,
  and running a copy from another path, `TCCAccessPreflight` still
  reported granted. The SPI symbols resolve (they report 2, "not
  determined", for a fresh client). Unverified until a signed build.
