# Microphone Capability — Contract

## What this is

Microphone capture for the app's webview, through the **standard web
APIs**: `navigator.mediaDevices.getUserMedia({ audio: true })` plus
`MediaRecorder`. There is no `desktop.microphone` JS API — an app records
exactly the way a web page does. What Chain adds is the part a web page
can't do for itself: declaring microphone use to the OS. Requested by
mneme (request 18, Phase 16 "Audio Recording").

There is no `contract.ts`: the structural contract is the web platform's
own, plus the declaration below.

## Declaring microphone use

In the app's own `package.json` (never in `.chain/native/`):

```json
"chain": {
  "permissions": {
    "microphone": "Mneme records lectures and notes you choose to capture."
  }
}
```

The value is the sentence the OS shows in its permission prompt. It must
be a non-empty string; an unknown key under `chain.permissions` fails
`chain dev`/`chain build` with an error naming the known keys.

`chain dev` and `chain build` read it on every run and emit:

- macOS: `NSMicrophoneUsageDescription` in `.chain/native/Info.plist`
  (Tauri merges that file into the dev binary and the release bundle),
  and `com.apple.security.device.audio-input` in
  `.chain/native/Entitlements.plist`, passed to `tauri build` so a
  signed, hardened-runtime build keeps microphone access.
- Windows: nothing is needed at build time. WebView2 shows its own
  permission prompt (see research/WINDOWS.md — unverified).

Without the declaration nothing is emitted, and on macOS `getUserMedia`
rejects `NotAllowedError`.

## Behavior an app can rely on

- `getUserMedia({ audio: true })` asks the OS (TCC on macOS) the first
  time; the OS prompt is the only gate. The webview never shows a second
  prompt of its own on macOS and never auto-denies.
- A refusal (now or earlier, in System Settings) rejects with the web
  API's own `NotAllowedError`. Chain doesn't wrap it in a `ChainError`.
- `MediaRecorder` with no `mimeType` produces:
  - macOS (WKWebView): `audio/mp4; codecs=mp4a.40.2` — AAC in an MP4
    container, 48 kHz mono (verified). `audio/webm;codecs=opus` also
    reports as supported. `audio/ogg` doesn't.
  - Windows (WebView2): expected `audio/webm;codecs=opus` (unverified).
    Apps store whatever `recorder.mimeType` says rather than assuming one.
- `pause()`/`resume()` work, and the paused span is left out of the
  recording.

## Development caveat

Under `chain dev` the app binary is started by whatever terminal or
editor ran `chain dev`, and macOS would attribute the microphone request
to _that_ app — `NotAllowedError` with no prompt. `chain dev` builds
relaunch themselves with that responsibility disclaimed
(`chain_core::dev_launch`, mneme request 22), so the dev app prompts with
its own name and usage sentence and gets its own row in Privacy &
Security → Microphone, exactly like the built app. Two differences
remain: the dev row is separate from the built app's (it's keyed by the
dev binary's path, not the bundle identifier), and `tccutil reset
Microphone <bundle id>` doesn't reach it — remove the row in System
Settings to be asked again. See agent-docs/framework/command/README.md
("Permission prompts under `chain dev`").

## Non-goals

- No native recorder (`desktop.audio.*`). The request's fallback wasn't
  needed — the webview route works. Revisit only if a real requirement
  needs recording without a window, or a format the webview can't make.
- No camera. Adding it is a new key (`camera`) plus
  `NSCameraUsageDescription`/`com.apple.security.device.camera`, when an
  app actually needs it.
- No device picking, level metering or format conversion — the web APIs
  (`enumerateDevices`, Web Audio `AnalyserNode`) already cover the first
  two in the webview.
