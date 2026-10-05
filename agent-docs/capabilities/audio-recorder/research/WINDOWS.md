# Audio recorder — Windows research

Not implemented: `availability()` reports every source `false` and
`start` rejects `UNSUPPORTED`. The plan, unverified:

## Capture (WASAPI, through the `windows` crate — no .NET)

- **Computer audio**: `IAudioClient::Initialize` on the default render
  endpoint (`eRender`, `eConsole`) with `AUDCLNT_STREAMFLAGS_LOOPBACK`,
  shared mode, event-driven (`AUDCLNT_STREAMFLAGS_EVENTCALLBACK`), read
  with `IAudioCaptureClient`. Available since Vista; no permission
  prompt. Loopback delivers nothing while nothing plays — fill silence
  from the device position / a timer so "both" and the duration stay
  right. Per-process include/exclude loopback
  (`ActivateAudioInterfaceAsync` + `AUDIOCLIENT_ACTIVATION_PARAMS`,
  Windows 10 2004+) could exclude the app's own sound if ever needed.
- **Microphone**: the default capture endpoint (`eCapture`). Refused
  access in Settings → Privacy → Microphone fails `Initialize`/`Start`
  with `E_ACCESSDENIED` → `PERMISSION_DENIED`.
- **Both**: two clients on two clocks at possibly different rates (mix
  format is usually 48 kHz float, but not always). Needs our own
  resampling to one rate and a small drift-tolerant buffer before
  `Mixer` — the work macOS's aggregate device does for free.
- `AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM` can ask the engine for one rate
  for both clients, which might remove the resampler.
- **Echo cancellation, noise suppression, gain control** are the same
  Rust code (`microphone_processor.rs`). Echo cancellation uses
  the loopback stream as the reference. It needs that resampling and
  drift handling done first: the two blocks must be sample-aligned on
  one clock, as on macOS (research/PROCESSING.md).
  `availability().echoCancellation` follows `both`; `noiseSuppression`
  and `autoGainControl` follow `microphone`.

## Choosing the microphone (request 35)

- List: `IMMDeviceEnumerator::EnumAudioEndpoints(eCapture,
  DEVICE_STATE_ACTIVE)`. id = `IMMDevice::GetId` (stable endpoint id),
  name = `PKEY_Device_FriendlyName`, default =
  `GetDefaultAudioEndpoint(eCapture, eConsole)`, rate = the mix format.
- Transport: `PKEY_AudioEndpoint_FormFactor` doesn't say Bluetooth.
  Use the device's enumerator (`PKEY_Device_EnumeratorName`, or the
  instance id prefix): `BTHENUM` / `BTHHFENUM` / `BTHLEDEVICE` →
  "bluetooth", `USB` → "usb"; an internal microphone-array form factor
  on the `HDAUDIO`/`INTELAUDIO` buses → "built-in"; else "other". Verify
  on real hardware.
- Opening a Bluetooth headset's capture endpoint switches it to its
  hands-free profile, as on macOS, so avoid-Bluetooth applies the same.
- Record from the chosen endpoint by id (never `SetDefaultEndpoint`).
  `IMMNotificationClient::OnDeviceStateChanged`/`OnDeviceRemoved` for
  that id → reopen on the replacement and send the same change event;
  ignore `OnDefaultDeviceChanged`. A new rate goes through the same Rust
  `RateConverter`.

## Encoding

`m4a.rs`'s `M4aWriter` is macOS-only. Same plan as tts's `compile`
(`agent-docs/capabilities/tts/research/WINDOWS.md`): Media Foundation's
sink writer to `.m4a`, AAC mono. The AAC encoder takes only 44.1/48 kHz,
which WASAPI's mix format nearly always is.

## Verification checklist (for whoever has Windows)

1. Computer audio of a playing video, 30 s: plays back, room silent.
2. Both while talking over a video; both audible.
3. Microphone refused in Settings → `PERMISSION_DENIED`.
4. Pause span left out; duration matches.
5. Nothing playing for a while during computer audio: the recording
   keeps its length (silence filled in).
6. Both on laptop speakers, talking over a video, `echoCancellation`
   on, then off: with it off the video is doubled, with it on it's heard
   once and the voice is kept.
7. Microphone with a fan running, `noiseSuppression` on vs off; a quiet
   voice far from the mic, `autoGainControl` on vs off.
8. `microphones()` with a USB and a Bluetooth headset connected:
   transports right. Bluetooth headset as default +
   `avoidBluetoothMicrophone` → built-in mic, headset audio stays in
   stereo/high quality. Unplug the chosen USB mic mid-take → event, take
   continues.
