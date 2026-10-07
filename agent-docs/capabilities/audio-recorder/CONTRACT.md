# AudioRecorder Capability — Contract

## What this is

`desktop.audioRecorder` records the microphone, the computer's own
output ("system"), or both mixed into one track, straight to an AAC file
on disk. Requested by mneme (request 32) to record lectures, calls and
videos playing on the laptop, which a webview can't reach. Structural
contract: `capabilities/audio-recorder/contract.ts`.

## Declaring it

In the app's own `package.json`, like the webview microphone:

```json
"chain": {
  "permissions": {
    "microphone": "Mneme records lectures and notes you choose to capture.",
    "systemAudio": "Mneme records the lectures and calls your computer plays."
  }
}
```

`microphone` covers "microphone"; `systemAudio` covers "system"; "both"
needs the two. On macOS they become `NSMicrophoneUsageDescription` and
`NSAudioCaptureUsageDescription`.

## Methods

- **`availability()`** — `{ microphone, system, both, echoCancellation,
  noiseSuppression, autoGainControl, microphoneChoice }`.
  A source is `true` when this platform and OS version can record it and
  the app declares its permission(s). It says nothing about whether the
  user has granted access. macOS: microphone wherever an input device
  exists; system and both from macOS 14.2. `echoCancellation` is `true`
  wherever "both" is; `noiseSuppression` and `autoGainControl` wherever
  "microphone" is (see "Processing the microphone" below), and so is
  `microphoneChoice` ("Choosing the microphone"). Windows: all `false`
  for now.
- **`microphones()`** — the connected inputs: `{ id, name, transport,
  isDefault, sampleRate }`, `transport` being `"built-in"`,
  `"bluetooth"`, `"usb"` or `"other"`. Empty where `microphoneChoice` is
  false.
- **`start({ source, microphone, avoidBluetoothMicrophone,
  echoCancellation, noiseSuppression, autoGainControl, onLevel,
  onMicrophoneChange })`** — asks for any permission the OS hasn't been
  given an answer for yet (the prompt shows the declared sentence),
  starts recording, and resolves `{ microphone, bluetoothFallback }`:
  the microphone actually recording (absent for "system"). One recording
  at a time, app-wide.
- **`pause()` / `resume()`** — the paused span is left out of the file
  and the duration. `onLevel` stops while paused. Pausing while paused,
  or resuming while recording, is a no-op.
- **`stop()`** — finishes the file and resolves `{ file, mimeType,
  durationMs }`: a `desktop.files` reference to an `.m4a` (AAC, mono,
  64 kbps, at the input device's rate), `mimeType` `"audio/mp4"`, and
  the recorded time in milliseconds.
- **`cancel()`** — stops and deletes the take. No-op when idle.

`onLevel(level)` arrives about ten times a second: the peak of what's
being recorded over that tenth of a second, on a dB scale where 0 is
−60 dBFS or quieter and 1 is full scale — suited to drawing a waveform.

The file is written as it records, so a three-hour recording never sits
in memory (~0.5 MB a minute). Until `stop()` it lives in a temp file
and has no reference, so a failed or cancelled take leaves nothing in
`desktop.files`.

A page load (reload, navigation) cancels a recording left running,
since the page that could stop it is gone.

## What "system" and "both" capture

- Everything the computer plays, from every app — **including the app's
  own sound** (mneme's read-aloud). Playback isn't muted: the user keeps
  hearing it.
- "both" balances the two automatically — each source is slowly levelled
  toward the same loudness, so a quiet voice over a loud lecture stays
  audible — then summed and soft-limited. With echo cancellation on, the
  microphone is only turned up while someone is speaking into it; in
  pauses it stays at its own level or lower, so echo the canceller left
  behind is never made louder.
- "microphone" and "system" alone are recorded as they arrive, no gain.

## Echo cancellation ("both")

Without headphones the microphone also picks up what the speakers play,
so the mix would hold the computer's sound twice, tens of milliseconds
apart. With `echoCancellation` on, the computer's sound is removed from
the microphone before mixing, using what the computer played as the
reference:

- **Default on.** `echoCancellation: false` records the microphone as it
  arrives (useful with headphones, where there's no echo to remove).
- **Ignored for "microphone" and "system"**: with one source there's no
  second copy to remove, so the option has no effect and isn't an error
  — an app can pass the same options whatever the source.
- It adapts on its own to the speaker-to-microphone delay, the volume
  and the room, within the first seconds of sound; moving the laptop or
  changing the volume makes it re-adapt briefly, so a trace of echo can
  return for a moment.
- The user's own voice is kept, including over the computer's sound. A
  faint trace of the echo can remain while both talk at once.
- In the first seconds of computer sound, and briefly after the volume
  changes, the canceller is still adapting, so some echo can be heard.
- It handles delays up to about 0.2 s between the computer playing a
  sound and the microphone hearing it. Wireless speakers can lag more,
  and then the echo isn't removed.
- Level, pause, resume, stop and cancel are unchanged; `onLevel`
  reflects the cleaned mix. It runs in real time at fixed memory, for
  recordings of any length.

## Choosing the microphone

For "microphone" and "both" (ignored for "system"):

- **`microphone`** — an id from `microphones()`. The recording uses that
  device, and the system's default input isn't changed. An id that isn't
  connected rejects with `UNAVAILABLE`, so the app can tell the user
  instead of silently recording elsewhere.
- Without it, the system's default input at `start` is used.
- **`avoidBluetoothMicrophone`** (default false) — when no `microphone`
  is given and the default input is a Bluetooth headset's, record from
  the built-in microphone instead (else another non-Bluetooth input).
  Opening a headset's microphone switches it to its call profile:
  narrowband, mono, compressed sound for the recording *and* for
  everything the user hears, and in "both" the computer's audio is
  recorded at that low rate too (the recording runs at its microphone's
  rate). Avoiding it keeps the headset at full quality, and headphones
  plus the built-in microphone also means "both" has no echo at all. If
  only a Bluetooth input exists it's used anyway and `bluetoothFallback`
  is true. An explicit `microphone` always wins: asking for a Bluetooth
  mic by id gets it.
- `transport` tells the app when a take is on a Bluetooth mic either way,
  so it can warn about call quality.

During a recording:

- **A microphone that connects, or a new default input, doesn't move the
  take.** The device chosen at `start` keeps recording.
- **If that microphone disappears** (unplugged, out of range), the
  recording moves to another one, chosen as with
  `avoidBluetoothMicrophone` (the built-in first). It keeps going without
  a gap, and `onMicrophoneChange({ microphone, previous,
  bluetoothFallback })` says so. A new microphone at another rate is
  converted to the take's rate (one rate per file; a simple conversion,
  fine for a voice). Echo cancellation re-adapts within seconds.
- For "system", if the output device the recording is timed by
  disappears, it moves to the new default output without an event: what
  the computer plays is still captured.

## Processing the microphone (noise, gain)

Two more options act on the microphone before mixing, in "microphone"
and "both". They're **ignored for "system"**, and the computer's audio
is never processed. Both are **off by default**, so a recording sounds
as it did unless the app opts in.

- **`noiseSuppression`** — reduces steady background noise (fans, hum,
  hiss, air conditioning) by about 15 dB. Sudden sounds such as typing
  or a door aren't removed. The strength is fixed: stronger suppression
  makes voices sound processed.
- **`autoGainControl`** — for "microphone": brings a quiet or distant
  voice up to an even level, by up to about 18 dB, a little at a time so
  it doesn't jump on every word. Without the computer's sound to compare
  against it can't tell a voice from steady noise, so it raises the
  background noise along with the voice. Turning on `noiseSuppression`
  with it is recommended. For "both" it adds nothing: the microphone is
  always evened out against the computer's sound there (above), only
  while someone speaks.
- All options together still run in real time at fixed memory.

## Errors (`ChainError.code`)

- `PERMISSION_DENIED` — the user refused microphone or computer-audio
  access, now or earlier. `message` names which. mneme can point to
  System Settings → Privacy & Security → Microphone, or → Screen &
  System Audio Recording ("System Audio Recording Only").
- `UNSUPPORTED` — outside a Chain app, on a platform/OS without it, or
  the app doesn't declare the source's permission (check
  `availability()` first).
- `UNAVAILABLE` — `start` while a recording is running; `pause`,
  `resume` or `stop` with nothing recording.
- `INVALID_ARGUMENT` — an unknown `source`.
- `NATIVE_FAILURE` — the device couldn't start, encoding failed (e.g. a
  full disk), or nothing was recorded.

## Relation to the microphone capability

The webview route (`getUserMedia` + `MediaRecorder`, see
`agent-docs/capabilities/microphone/`) keeps working. An app may record
microphone-only through either; this one gives one path for all three
sources, and an `.m4a` on macOS either way. Where `availability()` says
`microphone: false` (Windows, for now) the webview route is the
fallback.

## Non-goals

- Separate tracks per source, or rebalancing after recording.
- Recording or excluding one particular app's sound.
- Picking the output device, or recording one app's output.
- Moving a running take to a microphone the user chooses mid-recording
  (stop and start a new take); only a disappearing microphone moves it.
- A noise-suppression strength, removing sudden noises (typing,
  clicks), dereverberation (not functional in the library used), or
  voice activity detection as an app-facing feature — no real
  requirement yet.
- Processing the computer's audio ("system", or its side of "both").
- Echo cancellation for "microphone" alone: there's no reference, since
  the sound the speakers play isn't being captured.
- Formats other than AAC/`.m4a`.
- Video or screen capture.
- Recovering a take after a crash: the `.m4a` index is written by
  `stop()`, so a crash mid-recording loses it.
