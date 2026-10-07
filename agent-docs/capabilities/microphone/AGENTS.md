# Microphone Capability — Agent Memory

Scope: making the webview's own `getUserMedia` + `MediaRecorder` work in
a Chain app. Requested by mneme (`docs/chain-sdk-requests/18-microphone-capture.md`
in the mneme repo) for Phase 16 "Audio Recording".

## What's already decided

- **Web APIs, not a native recorder.** The request preferred this, and it
  works on macOS (verified below), so its `desktop.audio` fallback isn't
  built. Don't add one without a new real requirement.
- **Declared in the app's `package.json`, not `tauri.conf.json`.**
  `.chain/native/` is framework-owned and apps are told not to edit it;
  `package.json` is theirs. Generated at `chain dev`/`chain build` time,
  so an edit takes effect on the next run without `chain update`.
- **Entitlements go through `--config` on `tauri build`**, not into the
  tracked `tauri.conf.json`: pointing `bundle.macOS.entitlements` at a
  file that only exists when a permission is declared would break builds
  of apps that declare none.
- `speechRecognition` shares the same mechanism (request 19 — see
  `agent-docs/capabilities/speech/`).

## Status

- [x] macOS: verified in a bundled `apps/playground` build launched with
      `open` — TCC prompt named the app, Allow → 3.8 s AAC recording
      (`audio/mp4; codecs=mp4a.40.2`, 48 kHz mono) with pause/resume,
      stored through `desktop.files.write`, confirmed with `afinfo`.
- [x] `permissions.ts` generation, escaping, removal, unknown-key error —
      exercised against a scratch app folder.
- [ ] Signed, hardened-runtime build (needs a Developer ID) — confirm the
      entitlement really reaches the signature.
- [ ] Windows/WebView2: confirm its prompt appears and `MediaRecorder`
      gives `audio/webm;codecs=opus` (research/WINDOWS.md).
- [ ] Denial path in a bundled build (`NotAllowedError`) — only observed
      under `chain dev` so far, where the host terminal was refused.
