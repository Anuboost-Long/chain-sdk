# Microphone — macOS research

## WebKit's permission request

WKWebView asks its `WKUIDelegate`
`webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:`
before capturing. wry 0.55.1 (what Tauri 2.11 pins) implements it in
`src/wkwebview/class/wry_web_view_ui_delegate.rs` and always answers
`WKPermissionDecision::Grant` — for every origin and for camera too. So
there is no WebKit-level prompt and nothing for Chain to patch. (wry 0.57
routes it through a permission handler instead; recheck on upgrade.)

After the delegate grants, WebKit asks TCC, which needs
`NSMicrophoneUsageDescription` in the app's Info.plist — without it the
request is refused (or the process killed on direct AVFoundation use).

## Getting the key into the app

- `tauri dev`: `tauri-codegen`'s `generate_context!` embeds
  `<config dir>/Info.plist` into the binary's `__TEXT,__info_plist`
  section (verified with `launchctl plist __TEXT,__info_plist <binary>`).
  It's a proc macro, so Cargo doesn't notice a changed plist —
  `permissions.ts` touches `src/lib.rs` when it rewrites the file.
- `tauri build`: the bundler merges the same `Info.plist` into
  `Contents/Info.plist` (verified with PlistBuddy).
- Entitlement `com.apple.security.device.audio-input` matters only under
  the hardened runtime (signed builds). Local builds are ad-hoc/linker
  signed with no entitlements and record fine.

## TCC attribution (the dev-mode trap)

TCC charges the request to the _responsible process_. A binary started
from a terminal or editor inherits that app as responsible. In this
repo's test the host was Lazify (it has a usage description but no
audio-input entitlement): `getUserMedia` rejected `NotAllowedError`
immediately, with no prompt and no TCC log line, even though the
playground binary's embedded plist was correct.

Launched with `open playground.app` (LaunchServices → own responsible
process), the prompt named "playground" and Allow worked.

### Fix for `chain dev`: disclaimed relaunch (mneme request 22)

`chain_core::dev_launch::become_responsible_for_itself()` runs first in
a `chain dev` build's `run()`: if
`responsibility_get_pid_responsible_for_pid(getpid()) != getpid()`, it
`posix_spawn`s its own binary with
`responsibility_spawnattrs_setdisclaim(attr, 1)` and waits. Chosen over
wrapping the binary in a throwaway `.app` + `open`: no second bundle to
keep in sync with the Rust rebuild loop, stdio is simply inherited (log
streaming unchanged), and `tauri dev` still owns the pid it started.

Verified 2026-09-29 (macOS 26.6.2, Apple Silicon), playground under
`tauri dev --features chain-dev-inspector` — the same build `chain dev`
makes — started from Lazify:

- Waiter pid → responsible = Lazify; relaunched pid → responsible =
  itself. tccd's `AUTHREQ_ATTRIBUTION` names the playground binary as
  responsible for the `com.apple.WebKit.GPU` request.
- Prompt named "playground" with the Info.plist sentence; Allow → a
  live `getUserMedia` track; row listed in Privacy & Security.
- Grant kept across Rust rebuilds (no re-prompt). Stored as
  `identifier_type=Path` (the dev binary's path), so `tccutil reset
Microphone <id>` can't target it (it takes bundle ids only).
- Removing the row, then "Don't Allow" → `NotAllowedError`, tccd
  `authValue=0, authReason=2` (user choice). Switching the row _off_ in
  Settings instead was not honored after the next relaunch: macOS
  created a new entry and prompted again.
- `tauri dev` SIGKILLs the waiter on each Rust rebuild; the relaunched
  app exits with it (pipe EOF), no orphaned window.
- Trap while testing: WebKit parks `getUserMedia` while
  `document.visibilityState === "hidden"` — the promise neither
  resolves nor rejects until the window is visible.

## Verification run (2026-09-28, macOS 26.6.2, Apple Silicon)

`npx tauri build --debug --bundles app --features chain-dev-inspector
--config '{"bundle":{"macOS":{"entitlements":"Entitlements.plist"}}}'`
in `apps/playground`, `open` the bundle, then through the dev inspector:

- `MediaRecorder.isTypeSupported`: `audio/mp4` ✓, `audio/mp4;codecs=mp4a.40.2` ✓,
  `audio/webm;codecs=opus` ✓, `audio/webm` ✓, `audio/ogg` ✗.
- Default `recorder.mimeType`: `audio/mp4; codecs=mp4a.40.2`.
- 3 s record + 0.5 s pause + 1 s → 86 KB, `ftypiso5`, `afinfo`: AAC,
  1 ch, 48 kHz, 3.77 s (pause excluded).
